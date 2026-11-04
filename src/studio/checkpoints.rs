use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};
struct Job {
    generation: u64,
    path: PathBuf,
    saved: Project,
    project: Project,
    ctx: egui::Context,
}
pub(super) struct Checkpoints {
    generation: Arc<AtomicU64>,
    gate: Arc<Mutex<()>>,
    sender: Sender<Job>,
    receiver: Receiver<Result<(), String>>,
}
impl Default for Checkpoints {
    fn default() -> Self {
        let generation = Arc::new(AtomicU64::new(0));
        let gate = Arc::new(Mutex::new(()));
        let (sender, jobs) = mpsc::channel::<Job>();
        let (results, receiver) = mpsc::channel();
        let current = generation.clone();
        let lock = gate.clone();
        std::thread::spawn(move || {
            while let Ok(job) = jobs.recv() {
                let result = (|| -> anyhow::Result<()> {
                    if current.load(Ordering::Acquire) != job.generation {
                        return Ok(());
                    }
                    let snapshot = opengalgame::recovery::Snapshot {
                        version: 1,
                        path: opengalgame::recovery::normalized(&job.path),
                        disk_fingerprint: fingerprint(&job.saved),
                        created: stamp(),
                        project: job.project,
                    };
                    let bytes = serde_json::to_vec_pretty(&snapshot)?;
                    let path = opengalgame::recovery::recovery_path(&job.path);
                    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if current.load(Ordering::Acquire) == job.generation {
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let temp = path.with_extension(format!(
                            "{}.{}.tmp",
                            std::process::id(),
                            job.generation
                        ));
                        std::fs::write(&temp, bytes)?;
                        std::fs::rename(temp, path)?;
                    }
                    Ok(())
                })()
                .map_err(|e| e.to_string());
                if results.send(result).is_err() {
                    break;
                }
                job.ctx.request_repaint();
            }
        });
        Self {
            generation,
            gate,
            sender,
            receiver,
        }
    }
}
impl Checkpoints {
    pub fn schedule(&self, path: &Path, saved: &Project, project: &Project, ctx: &egui::Context) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.sender.send(Job {
            generation,
            path: path.to_owned(),
            saved: saved.clone(),
            project: project.clone(),
            ctx: ctx.clone(),
        });
    }
    pub fn poll(&self) -> Option<Result<(), String>> {
        self.receiver.try_iter().last()
    }
    pub fn dismiss(&self, path: &Path, saved: &Project) -> anyhow::Result<()> {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        opengalgame::recovery::dismiss(path, saved)
    }
}
impl Drop for Checkpoints {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> (Project, PathBuf) {
        let p = opengalgame::catalog::template_project(
            "恢复测试",
            opengalgame::catalog::ProjectTemplate::Blank,
        );
        let dir = std::env::temp_dir().join(&p.id);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.json");
        save_project(&path, &p).unwrap();
        (p, path)
    }
    fn wait(worker: &Checkpoints) {
        let start = Instant::now();
        loop {
            if let Some(result) = worker.poll() {
                result.unwrap();
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn background_checkpoint_keeps_disk_and_other_projects_independent() {
        let (a, pa) = project();
        let (b, pb) = project();
        let wa = Checkpoints::default();
        let wb = Checkpoints::default();
        let mut changed = a.clone();
        changed.nodes[0].text = "甲未保存内容".into();
        wa.schedule(&pa, &a, &changed, &egui::Context::default());
        wb.schedule(&pb, &b, &b, &egui::Context::default());
        wait(&wa);
        wait(&wb);
        assert_eq!(
            opengalgame::recovery::candidate(&pa, &a).unwrap().project,
            changed
        );
        assert!(opengalgame::recovery::candidate(&pb, &b).is_none());
        assert_eq!(read_project(&pa).unwrap(), a);
        wa.dismiss(&pa, &a).unwrap();
    }
    #[test]
    fn dismiss_invalidates_queued_checkpoint_so_discarded_edits_cannot_reappear() {
        let (p, path) = project();
        let worker = Checkpoints::default();
        let mut changed = p.clone();
        changed.nodes[0].text = "应当放弃".repeat(5000);
        worker.schedule(&path, &p, &changed, &egui::Context::default());
        worker.dismiss(&path, &p).unwrap();
        wait(&worker);
        assert!(opengalgame::recovery::candidate(&path, &p).is_none());
    }
}
