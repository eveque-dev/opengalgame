use super::*;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

enum Entry {
    Pending,
    Ready {
        texture: egui::TextureHandle,
        bytes: usize,
        touched: u64,
    },
    Failed,
}
struct Request {
    key: String,
    base: PathBuf,
    file: String,
    size: u32,
    ctx: egui::Context,
}
struct Decoded {
    key: String,
    result: Result<image::RgbaImage, String>,
}
pub(super) struct Images {
    entries: HashMap<String, Entry>,
    sender: SyncSender<Request>,
    receiver: Receiver<Decoded>,
    bytes: usize,
    tick: u64,
}
impl Default for Images {
    fn default() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Request>(8);
        let (results, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(job) = jobs.recv() {
                let result = (|| -> anyhow::Result<_> {
                    let path = safe_asset(&job.base, &job.file)?;
                    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(8192);
                    limits.max_image_height = Some(8192);
                    limits.max_alloc = Some(256 * 1024 * 1024);
                    reader.limits(limits);
                    let image = reader.decode()?;
                    anyhow::ensure!(
                        image.width() <= 8192 && image.height() <= 8192,
                        "图片超过 8192 像素"
                    );
                    Ok(image.thumbnail(job.size, job.size).into_rgba8())
                })()
                .map_err(|e| e.to_string());
                if results
                    .send(Decoded {
                        key: job.key,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                job.ctx.request_repaint();
            }
        });
        Self {
            entries: HashMap::new(),
            sender,
            receiver,
            bytes: 0,
            tick: 0,
        }
    }
}
impl Images {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(decoded) = self.receiver.try_recv() {
            if !matches!(self.entries.get(&decoded.key), Some(Entry::Pending)) {
                continue;
            }
            let entry = match decoded.result {
                Ok(rgba) => {
                    let bytes = rgba.len();
                    let texture = ctx.load_texture(
                        &decoded.key,
                        egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    );
                    self.bytes += bytes;
                    Entry::Ready {
                        texture,
                        bytes,
                        touched: self.tick,
                    }
                }
                Err(_) => Entry::Failed,
            };
            self.entries.insert(decoded.key, entry);
            while self.bytes > 96 * 1024 * 1024 || self.entries.len() > 160 {
                let oldest = self
                    .entries
                    .iter()
                    .filter_map(|(k, e)| match e {
                        Entry::Ready { touched, .. } => Some((k.clone(), *touched)),
                        Entry::Failed => Some((k.clone(), 0)),
                        Entry::Pending => None,
                    })
                    .min_by_key(|(_, t)| *t)
                    .map(|(k, _)| k);
                let Some(oldest) = oldest else {
                    break;
                };
                if let Some(Entry::Ready { bytes, .. }) = self.entries.remove(&oldest) {
                    self.bytes = self.bytes.saturating_sub(bytes);
                }
            }
        }
    }
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        base: &Path,
        file: &str,
        size: u32,
    ) -> Option<egui::TextureHandle> {
        self.poll(ctx);
        self.tick += 1;
        let key = format!("{}|{file}|{size}", base.display());
        if let Some(entry) = self.entries.get_mut(&key) {
            return match entry {
                Entry::Ready {
                    texture, touched, ..
                } => {
                    *touched = self.tick;
                    Some(texture.clone())
                }
                _ => None,
            };
        }
        match self.sender.try_send(Request {
            key: key.clone(),
            base: base.to_owned(),
            file: file.to_owned(),
            size,
            ctx: ctx.clone(),
        }) {
            Ok(()) => {
                self.entries.insert(key, Entry::Pending);
            }
            Err(TrySendError::Full(_)) => ctx.request_repaint_after(Duration::from_millis(30)),
            Err(TrySendError::Disconnected(_)) => {
                self.entries.insert(key, Entry::Failed);
            }
        }
        None
    }
}
