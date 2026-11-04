use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use std::path::Path;

#[derive(Default)]
pub struct SoundEngine {
    stream: Option<OutputStream>,
    music: Option<Sink>,
    effect: Option<Sink>,
    key: Option<String>,
    pub error: Option<String>,
}
impl SoundEngine {
    fn init(&mut self) -> Result<()> {
        if self.stream.is_none() {
            self.stream = Some(
                OutputStreamBuilder::open_default_stream().context("找不到可用的音频输出设备")?,
            );
        }
        Ok(())
    }
    pub fn music(&mut self, path: Option<&Path>, volume: f32) -> Result<()> {
        let key = path.map(|p| p.to_string_lossy().to_string());
        if key != self.key {
            if let Some(old) = self.music.take() {
                old.stop();
            }
            self.key = None;
            if let Some(path) = path {
                self.init()?;
                let source = rodio::Decoder::try_from(std::fs::File::open(path)?)?;
                let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
                sink.set_volume(volume.clamp(0., 1.));
                sink.append(source.repeat_infinite());
                self.music = Some(sink);
                self.key = key;
            }
        }
        if let Some(sink) = &self.music {
            sink.set_volume(volume.clamp(0., 1.));
        }
        Ok(())
    }
    pub fn effect(&mut self, path: &Path, volume: f32) -> Result<()> {
        self.init()?;
        let source = rodio::Decoder::try_from(std::fs::File::open(path)?)?;
        if let Some(old) = self.effect.take() {
            old.stop();
        }
        let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        sink.set_volume(volume.clamp(0., 1.));
        sink.append(source);
        self.effect = Some(sink);
        Ok(())
    }
    pub fn volumes(&mut self, music: f32, effects: f32) {
        if let Some(s) = &self.music {
            s.set_volume(music.clamp(0., 1.));
        }
        if let Some(s) = &self.effect {
            s.set_volume(effects.clamp(0., 1.));
        }
    }
    pub fn stop(&mut self) {
        if let Some(s) = self.music.take() {
            s.stop();
        }
        if let Some(s) = self.effect.take() {
            s.stop();
        }
        self.key = None;
    }
}
