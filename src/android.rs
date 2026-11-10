use crate::{bundle, project_io::read_project, studio::Studio};
use anyhow::{Context, Result};
use std::path::PathBuf;
use winit::platform::android::activity::AndroidApp;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    if let Err(error) = run(app) {
        eprintln!("OpenGalgame: {error:#}");
    }
}

fn run(app: AndroidApp) -> Result<()> {
    let data = app.internal_data_path().context("无法取得应用数据目录")?;
    std::fs::create_dir_all(&data)?;
    let _ = crate::paths::ANDROID_DATA.set(data.clone());
    let cache = data.join("../cache");
    std::fs::create_dir_all(&cache)?;
    let directory = tempfile::TempDir::with_prefix_in("game-", &cache)?;
    let assets = app.asset_manager();
    let archive = assets.open(c"game.zip").context("APK 缺少游戏素材")?;
    bundle::extract(archive, directory.path())?;
    let path = directory.path().join("game.json");
    let project = read_project(&path)?;
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        run_and_return: false,
        ..Default::default()
    };
    let title = project.title.clone();
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            Ok(Box::new(AndroidPlayer {
                studio: Studio::new(cc, project, path, true),
                _game_files: directory,
            }))
        }),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

struct AndroidPlayer {
    studio: Studio,
    _game_files: tempfile::TempDir,
}

impl eframe::App for AndroidPlayer {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.studio.update(ctx, frame);
    }
}

// The shared Studio enters player mode immediately. File pickers belong to
// its unreachable desktop editing branches.
pub(crate) mod editor_dialogs {
    use super::PathBuf;

    pub struct FileDialog;

    impl FileDialog {
        pub fn new() -> Self {
            Self
        }
        pub fn add_filter(self, _: impl Into<String>, _: &[&str]) -> Self {
            self
        }
        pub fn set_title(self, _: &str) -> Self {
            self
        }
        pub fn set_file_name(self, _: impl Into<String>) -> Self {
            self
        }
        pub fn pick_file(self) -> Option<PathBuf> {
            None
        }
        pub fn pick_folder(self) -> Option<PathBuf> {
            None
        }
        pub fn save_file(self) -> Option<PathBuf> {
            None
        }
    }
}
