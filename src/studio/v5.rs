use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Layout {
    pub outline: bool,
    pub inspector: bool,
    pub left_width: f32,
    pub right_width: f32,
    pub columns: [f32; 9],
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            outline: true,
            inspector: true,
            left_width: 232.,
            right_width: 332.,
            columns: [32., 165., 95., 150., 170., 350., 170., 190., 65.],
        }
    }
}
pub(super) struct V5 {
    pub layout: Layout,
    pub persisted: Layout,
    pub layout_at: Instant,
    pub preview: Option<(String, Instant)>,
    pub stage_drag: Option<(String, usize)>,
    pub frame: Option<opengalgame::production::StageFrame>,
    pub instant_scene: bool,
    pub voice_started: bool,
    pub sequence_complete: bool,
    pub font_key: String,
    pub font_family: egui::FontFamily,
    pub pending_font: Option<egui::FontFamily>,
    pub logic_open: bool,
    pub logic_choice: usize,
    pub report: Option<opengalgame::testing::TestReport>,
    pub report_rx: Option<std::sync::mpsc::Receiver<opengalgame::testing::TestReport>>,
    pub test_limit: usize,
    pub case_name: String,
    pub audit: Option<opengalgame::assets::Audit>,
    pub repair: Option<opengalgame::assets::RepairPlan>,
    pub voices: Option<opengalgame::assets::VoicePlan>,
    pub asset_old: String,
    pub asset_new: String,
    pub asset_kind: opengalgame::assets::AssetKind,
    pub rename_file: String,
    pub file_stem: String,
    pub export_preflight: bool,
}
impl V5 {
    pub fn layout_path(path: &Path) -> PathBuf {
        use sha2::{Digest, Sha256};
        let identity = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_owned())
            .to_string_lossy()
            .to_lowercase();
        let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
        opengalgame::paths::data_directory(format!("layouts/{}.json", &hash[..20]))
    }
    pub fn new(path: &Path) -> Self {
        let mut layout: Layout = std::fs::read(Self::layout_path(path))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if !layout.left_width.is_finite()
            || !layout.right_width.is_finite()
            || layout.columns.iter().any(|c| !c.is_finite())
        {
            layout = Layout::default();
        }
        layout.left_width = layout.left_width.clamp(205., 500.);
        layout.right_width = layout.right_width.clamp(285., 900.);
        for width in &mut layout.columns {
            *width = width.clamp(32., 800.);
        }
        Self {
            persisted: layout.clone(),
            layout,
            layout_at: Instant::now(),
            preview: None,
            stage_drag: None,
            frame: None,
            instant_scene: false,
            voice_started: false,
            sequence_complete: true,
            font_key: String::new(),
            font_family: egui::FontFamily::Proportional,
            pending_font: None,
            logic_open: false,
            logic_choice: 0,
            report: None,
            report_rx: None,
            test_limit: 2000,
            case_name: "新的回归用例".into(),
            audit: None,
            repair: None,
            voices: None,
            asset_old: String::new(),
            asset_new: String::new(),
            asset_kind: opengalgame::assets::AssetKind::Character,
            rename_file: String::new(),
            file_stem: String::new(),
            export_preflight: false,
        }
    }
}
