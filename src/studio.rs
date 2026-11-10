#[cfg(target_os = "android")]
use crate::android::editor_dialogs as rfd;
use crate::editor_theme::{accent, text_on};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use opengalgame::{
    bundle::{self, Format},
    model::*,
    project_io::*,
    runtime::*,
};
mod advanced;
mod checkpoints;
mod debug_ui;
mod direction_ui;
mod images;
mod logic_ui;
mod production_player;
mod production_ui;
mod table_ui;
mod v4;
mod v5;
use advanced::Advanced;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const BASE: Color32 = Color32::from_rgb(21, 29, 43);
const PANEL: Color32 = Color32::from_rgb(32, 42, 57);
const FIELD: Color32 = Color32::from_rgb(41, 54, 74);
const INK: Color32 = Color32::from_rgb(230, 234, 243);
const MUTED: Color32 = Color32::from_rgb(168, 181, 201);
const ACCENT: Color32 = Color32::from_rgb(169, 156, 241);
const GOOD: Color32 = Color32::from_rgb(128, 200, 179);
const BAD: Color32 = Color32::from_rgb(255, 176, 181);
const WARN: Color32 = Color32::from_rgb(235, 199, 126);

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Script,
    Table,
    Characters,
    Backgrounds,
    Flags,
    Check,
    Project,
    Audio,
    Direction,
    Theme,
    Assets,
    Tests,
}
enum Pending {
    Open(PathBuf),
    New,
    Exit,
}
enum Action {
    Next,
    Choice(usize),
    Back,
    Restart,
}
#[cfg_attr(target_os = "android", allow(dead_code))]
pub(crate) enum WorkspaceRequest {
    Open(PathBuf),
    New,
    Export {
        project: Box<Project>,
        base: PathBuf,
        destination: PathBuf,
        format: Format,
    },
}
pub struct Studio {
    v4: v4::V4,
    v5: v5::V5,
    pub(crate) managed: bool,
    pub(crate) external_modal: bool,
    alias_old: String,
    alias_new: String,
    pub(crate) workspace_request: Option<WorkspaceRequest>,
    pub(crate) revision: u64,
    pub(crate) saved_generation: u64,
    dirty_flag: bool,
    images: images::Images,
    checkpoints: checkpoints::Checkpoints,
    filter_revision: u64,
    filter_query: String,
    filter_chapter: String,
    filter_rows: Vec<usize>,
    cycle_revision: u64,
    cycle_nodes: std::sync::Arc<std::collections::BTreeSet<String>>,
    play_fingerprint: String,
    target_revision: u64,
    target_cache: std::sync::Arc<Vec<(String, String)>>,
    project: Project,
    path: PathBuf,
    saved: Project,
    selected: usize,
    section: Section,
    graph: bool,
    runtime: Option<Runtime>,
    player_only: bool,
    textures: HashMap<String, Result<egui::TextureHandle, String>>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    last_edit: Instant,
    status: String,
    status_error: bool,
    issues: Vec<Issue>,
    search: String,
    node_id: String,
    new_flag: String,
    pending: Option<Pending>,
    skip_history: bool,
    saves_open: bool,
    log_open: bool,
    delete_node: bool,
    auto: bool,
    auto_started: Instant,
    text_size: f32,
    full_screen: bool,
    graph_zoom: f32,
    ui_scale: f32,
    help_open: bool,
    export_path: Option<PathBuf>,
    advanced: Advanced,
}
pub fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(windows)]
    let candidates = [r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\simhei.ttf"];
    #[cfg(target_os = "android")]
    let candidates = [
        "/system/fonts/NotoSansCJK-Regular.ttc",
        "/system/fonts/NotoSansSC-Regular.otf",
        "/system/fonts/NotoSansCJKsc-Regular.otf",
    ];
    for candidate in candidates {
        if let Ok(bytes) = std::fs::read(candidate) {
            fonts
                .font_data
                .insert("chinese".into(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "chinese".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BASE;
    style.visuals.faint_bg_color = FIELD;
    style.visuals.override_text_color = Some(INK);
    style.visuals.widgets.inactive.bg_fill = FIELD;
    style.visuals.widgets.inactive.weak_bg_fill = FIELD;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(60, 73, 99);
    style.visuals.selection.bg_fill = Color32::from_rgb(82, 72, 128);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.spacing.item_spacing = Vec2::new(10., 10.);
    style.spacing.button_padding = Vec2::new(13., 8.);
    style.spacing.interact_size.y = if cfg!(target_os = "android") {
        48.
    } else {
        34.
    };
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(16.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(15.));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(23.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(13.));
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
    ctx.set_theme(egui::Theme::Dark);
}
impl Studio {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        project: Project,
        path: PathBuf,
        player: bool,
    ) -> Self {
        configure(&cc.egui_ctx);
        Self::create(project, path, player)
    }
    pub(crate) fn create(project: Project, path: PathBuf, player: bool) -> Self {
        let runtime = None;
        let advanced = Advanced::new(&project, &path, player);
        let node_id = project
            .nodes
            .first()
            .map(|n| n.id.clone())
            .unwrap_or_default();
        Self {
            v4: Default::default(),
            v5: v5::V5::new(&path),
            managed: false,
            external_modal: false,
            alias_old: String::new(),
            alias_new: String::new(),
            workspace_request: None,
            revision: 0,
            saved_generation: 0,
            dirty_flag: false,
            images: Default::default(),
            checkpoints: Default::default(),
            filter_revision: u64::MAX,
            filter_query: String::new(),
            filter_chapter: String::new(),
            filter_rows: vec![],
            cycle_revision: u64::MAX,
            cycle_nodes: Default::default(),
            play_fingerprint: String::new(),
            target_revision: u64::MAX,
            target_cache: Default::default(),
            saved: project.clone(),
            project,
            path,
            selected: 0,
            section: Section::Script,
            graph: false,
            runtime,
            player_only: player,
            textures: HashMap::new(),
            undo: vec![],
            redo: vec![],
            last_edit: Instant::now() - Duration::from_secs(5),
            status: if player {
                "点击继续，或按空格开始阅读"
            } else {
                "项目已打开。修改右侧对白，立刻查看舞台效果。"
            }
            .into(),
            status_error: false,
            issues: vec![],
            search: String::new(),
            node_id,
            new_flag: String::new(),
            pending: None,
            skip_history: false,
            saves_open: false,
            log_open: false,
            delete_node: false,
            auto: false,
            auto_started: Instant::now(),
            text_size: advanced.settings.font,
            full_screen: false,
            graph_zoom: 0.8,
            ui_scale: 1.0,
            help_open: false,
            export_path: None,
            advanced,
        }
    }
    pub(crate) fn dirty(&self) -> bool {
        self.dirty_flag
    }
    #[cfg(windows)]
    pub(crate) fn project(&self) -> &Project {
        &self.project
    }
    #[cfg(windows)]
    pub(crate) fn selected_scene(&self) -> Option<&Node> {
        if self.player_only || self.runtime.is_some() {
            return None;
        }
        self.project.nodes.get(self.selected)
    }
    #[cfg(windows)]
    pub(crate) fn is_playing(&self) -> bool {
        self.runtime.is_some()
    }
    #[cfg(windows)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    #[cfg(windows)]
    pub(crate) fn pause(&mut self) {
        if !self.player_only {
            let _ = write_json(&v5::V5::layout_path(&self.path), &self.v5.layout);
        }
        self.v4.debug.running = false;
        self.auto = false;
        self.advanced.sound.stop();
        self.images.clear();
        self.textures.clear();
    }
    #[cfg(windows)]
    pub(crate) fn discard_recovery(&self) {
        let _ = self.checkpoints.dismiss(&self.path, &self.saved);
    }
    #[cfg(windows)]
    pub(crate) fn edit_external<T>(&mut self, edit: impl FnOnce(&mut Project) -> T) -> T {
        let before = self.project.clone();
        let result = edit(&mut self.project);
        if before != self.project {
            self.undo.push(before);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.revision += 1;
            self.dirty_flag = self.project != self.saved;
            if !self.dirty_flag {
                let _ = self.checkpoints.dismiss(&self.path, &self.saved);
            }
            self.issues.clear();
        }
        result
    }
    #[cfg(windows)]
    pub(crate) fn focus_scene(&mut self, id: &str) {
        if self.runtime.is_none()
            && let Some(index) = self.project.nodes.iter().position(|node| node.id == id)
        {
            self.select(index);
            self.section = Section::Script;
            self.graph = false;
        }
    }
    fn node_targets(&mut self) -> std::sync::Arc<Vec<(String, String)>> {
        if self.target_revision != self.revision {
            self.target_cache = std::sync::Arc::new(
                self.project
                    .nodes
                    .iter()
                    .map(|n| (n.id.clone(), n.title.clone()))
                    .collect(),
            );
            self.target_revision = self.revision;
        }
        self.target_cache.clone()
    }
    fn base(&self) -> &Path {
        self.path.parent().unwrap_or_else(|| Path::new("."))
    }
    fn message(&mut self, result: anyhow::Result<impl ToString>) {
        match result {
            Ok(v) => {
                self.status = v.to_string();
                self.status_error = false;
            }
            Err(e) => {
                self.status = format!("{e:#}");
                self.status_error = true;
            }
        }
    }
    fn select(&mut self, index: usize) {
        self.selected = index;
        self.node_id = self
            .project
            .nodes
            .get(index)
            .map(|n| n.id.clone())
            .unwrap_or_default();
    }
    pub(crate) fn save(&mut self) -> bool {
        match save_project(&self.path, &self.project) {
            Ok(()) => {
                self.saved = self.project.clone();
                let _ = self.checkpoints.dismiss(&self.path, &self.saved);
                self.dirty_flag = false;
                self.saved_generation += 1;
                self.message(Ok(format!("已保存到 {}", self.path.display())));
                true
            }
            Err(e) => {
                self.message(Err::<String, _>(e));
                false
            }
        }
    }
    fn check(&mut self) -> bool {
        self.issues = validate(&self.project, Some(self.base()));
        let errors = self
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        self.status = if self.issues.is_empty() {
            "校验通过：引用、素材与条件路径均有效。".into()
        } else {
            format!(
                "检查完成：{errors} 个错误，{} 个提示",
                self.issues.len() - errors
            )
        };
        self.status_error = errors > 0;
        errors == 0
    }
    fn begin(&mut self, from_selected: bool) {
        if self.advanced.recovery.is_some()
            || self.pending.is_some()
            || self.advanced.delete_asset.is_some()
        {
            return;
        }
        if !self.check() {
            self.section = Section::Check;
            return;
        }
        self.runtime = Some(if from_selected {
            Runtime::at(&self.project, &self.project.nodes[self.selected].id)
        } else {
            Runtime::new(&self.project)
        });
        if let Some(error) = self.runtime.as_ref().and_then(|r| r.error.clone()) {
            self.runtime = None;
            self.message(Err::<String, _>(anyhow::anyhow!(error)));
            return;
        }
        self.v5.instant_scene = false;
        if !self.player_only
            && let Some(runtime) = &self.runtime
        {
            self.v4.debug.start(&self.project, runtime);
            if from_selected {
                self.v4.debug.tainted = true;
            }
        }
        self.play_fingerprint = fingerprint(&self.project);
        self.advanced.scene_key.clear();
        self.advanced.bulk_open = false;
        self.advanced.replace_open = false;
        self.advanced.edge = None;
        self.advanced.title_screen = false;
        self.section = Section::Script;
        self.graph = false;
        self.auto = false;
        self.auto_started = Instant::now();
        self.message(Ok("试玩已开始，使用下方按钮或空格推进。"));
    }
    fn step(&mut self, action: Action) {
        if self.advanced.transitioning {
            return;
        }
        if matches!(action, Action::Next) && !self.graph && self.advanced.text_incomplete {
            self.advanced.revealed = true;
            return;
        }
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        let result = match action {
            Action::Next => runtime.advance(&self.project, None),
            Action::Choice(i) => {
                self.auto = false;
                runtime.advance(&self.project, Some(i))
            }
            Action::Back => {
                self.auto = false;
                if runtime.back() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("已在开头"))
                }
            }
            Action::Restart => {
                *runtime = Runtime::new(&self.project);
                self.auto = false;
                Ok(())
            }
        };
        if let Err(e) = result {
            self.message(Err::<String, _>(e));
            return;
        }
        self.v5.instant_scene = matches!(action, Action::Back);
        self.advanced.scene_key.clear();
        if !self.player_only {
            if matches!(action, Action::Restart) {
                self.v4.debug.start(&self.project, runtime);
            } else {
                let command = match action {
                    Action::Next => opengalgame::debugger::Command::Advance { choice: None },
                    Action::Choice(i) => {
                        opengalgame::debugger::Command::Advance { choice: Some(i) }
                    }
                    Action::Back => opengalgame::debugger::Command::Back,
                    Action::Restart => unreachable!(),
                };
                if let Some(route) = &mut self.v4.debug.route
                    && let Err(e) = route.record(command, runtime)
                {
                    self.v4.debug.message = e.to_string();
                }
                self.v4.debug.observe(&self.project, runtime);
                if self.v4.debug.breakpoints.contains(&runtime.current.node) {
                    self.auto = false;
                }
            }
        }
        self.auto_started = Instant::now();
        if self.player_only {
            self.save_slot("auto");
        }
    }
    fn save_slot(&mut self, slot: &str) {
        if !self.player_only && self.v4.debug.tainted {
            self.message(Ok("调试状态不写入玩家存档，请导出调试路线。"));
            return;
        }
        if let Some(r) = &self.runtime {
            let result = write_json(
                &save_directory(&self.project.id).join(format!("{slot}.json")),
                &r.save_with_fingerprint(&self.project, &self.play_fingerprint),
            );
            if result.is_ok() && slot != "auto" {
                self.advanced.capture = Some((slot.to_owned(), 2));
                self.saves_open = false;
            }
            if slot != "auto" || result.is_err() {
                self.message(result.map(|_| "进度已保存到当前用户的本机存档"));
            }
        }
    }
    fn load_slot(&mut self, slot: &str) {
        let result = (|| {
            let bytes =
                std::fs::read(save_directory(&self.project.id).join(format!("{slot}.json")))?;
            let save: SaveGame = serde_json::from_slice(&bytes)?;
            opengalgame::compatibility::load(&self.project, save)
        })();
        match result {
            Ok((r, report)) => {
                self.v5.instant_scene = true;
                if !self.player_only {
                    self.v4.debug.start(&self.project, &r);
                }
                self.play_fingerprint = fingerprint(&self.project);
                self.runtime = Some(r);
                self.advanced.title_screen = false;
                self.advanced.scene_key.clear();
                self.auto = false;
                self.auto_started = Instant::now();
                self.saves_open = false;
                self.message(Ok(report.description()));
            }
            Err(e) => self.message(Err::<String, _>(e)),
        }
    }
    fn request(&mut self, action: Pending) {
        if self.managed {
            match action {
                Pending::Open(path) => self.workspace_request = Some(WorkspaceRequest::Open(path)),
                Pending::New => self.workspace_request = Some(WorkspaceRequest::New),
                Pending::Exit => {}
            };
            return;
        }
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }
    fn perform(&mut self, action: Pending) {
        match action {
            Pending::Exit => {}
            Pending::Open(path) => match read_project(&path) {
                Ok(project) => {
                    self.project = project;
                    self.saved = self.project.clone();
                    self.path = path;
                    self.advanced = Advanced::new(&self.project, &self.path, false);
                    let _ = opengalgame::recovery::remember(&self.path);
                    self.select(0);
                    self.runtime = None;
                    self.issues.clear();
                    self.textures.clear();
                    self.images.clear();
                    self.undo.clear();
                    self.redo.clear();
                    self.skip_history = true;
                    self.message(Ok("项目已打开"));
                }
                Err(e) => self.message(Err::<String, _>(e)),
            },
            Pending::New => {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("选择新项目的父目录")
                    .pick_folder()
                {
                    let base = folder.join(format!("opengalgame-project-{}", stamp()));
                    let project = Project {
                        format_version: 1,
                        id: format!("project-{}", stamp()),
                        title: "我的视觉小说".into(),
                        author: String::new(),
                        start: "opening".into(),
                        characters: vec![],
                        backgrounds: vec![],
                        flags: Default::default(),
                        variables: Default::default(),
                        audio: vec![],
                        presentation: Default::default(),
                        tasks: vec![],
                        save_compatibility: Default::default(),
                        production: Default::default(),
                        story_recap: String::new(),
                        nodes: vec![Node {
                            text: "你的第一个故事，从这里开始。".into(),
                            ..Node::new("opening".into(), NodeKind::Ending, 0)
                        }]
                        .into(),
                    };
                    let path = base.join("project.json");
                    match save_project(&path, &project) {
                        Ok(()) => {
                            self.project = project.clone();
                            self.saved = project;
                            self.path = path;
                            self.advanced = Advanced::new(&self.project, &self.path, false);
                            let _ = opengalgame::recovery::remember(&self.path);
                            self.select(0);
                            self.section = Section::Script;
                            self.runtime = None;
                            self.textures.clear();
                            self.images.clear();
                            self.undo.clear();
                            self.redo.clear();
                            self.issues.clear();
                            self.skip_history = true;
                            self.message(Ok("新项目已创建。添加对白节点，再把它连接到结局。"));
                        }
                        Err(e) => self.message(Err::<String, _>(e)),
                    }
                }
            }
        }
    }
    fn history_edit(&mut self, redo: bool) {
        let value = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(p) = value {
            if redo {
                self.undo.push(self.project.clone());
            } else {
                self.redo.push(self.project.clone());
            }
            self.project = p;
            self.select(
                self.selected
                    .min(self.project.nodes.len().saturating_sub(1)),
            );
            self.skip_history = true;
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top(egui::Id::new(("toolbar", &self.path)))
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                    ui.painter().rect_filled(logo, 7, accent(ui));
                    ui.painter().line_segment(
                        [logo.min + Vec2::new(7., 7.), logo.min + Vec2::new(7., 21.)],
                        Stroke::new(2.0_f32, ui.visuals().extreme_bg_color),
                    );
                    ui.painter().line_segment(
                        [
                            logo.min + Vec2::new(7., 21.),
                            logo.min + Vec2::new(21., 21.),
                        ],
                        Stroke::new(2.0_f32, ui.visuals().extreme_bg_color),
                    );
                    ui.label(RichText::new("OpenGalgame").size(21.).strong());
                    ui.separator();
                    ui.add(
                        egui::Label::new(RichText::new(&self.project.title).color(MUTED).size(15.))
                            .truncate(),
                    )
                    .on_hover_text(&self.project.title);
                    if self.dirty() {
                        badge(ui, "未保存", accent(ui));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tool(ui, Icon::Export, "导出游戏", true)
                            .on_hover_text("导出 Windows x64 游戏目录，内含 Game.exe 和素材")
                            .clicked()
                        {
                            self.v5.audit =
                                opengalgame::assets::audit(&self.project, self.base()).ok();
                            self.v5.export_preflight = true;
                        }
                        if self.runtime.is_some() {
                            if tool(ui, Icon::Stop, "停止试玩", false).clicked() {
                                self.runtime = None;
                                self.auto = false;
                            }
                        } else if tool(ui, Icon::Play, "试玩", false)
                            .on_hover_text("F5 从头试玩；也可从当前节点开始")
                            .clicked()
                        {
                            self.begin(false);
                        }
                        if tool(ui, Icon::Check, "检查", false).clicked() {
                            self.check();
                            self.section = Section::Check;
                        }
                        if tool(ui, Icon::Save, "保存", false)
                            .on_hover_text("Ctrl+S 保存项目，上一版本保留为 .bak")
                            .clicked()
                        {
                            self.save();
                        }
                        if tool(ui, Icon::Folder, "打开", false).clicked()
                            && let Some(file) = rfd::FileDialog::new()
                                .add_filter("剧情项目", &["json"])
                                .pick_file()
                        {
                            self.request(Pending::Open(file));
                        }
                        if tool(ui, Icon::Add, "新建", false).clicked() {
                            self.request(Pending::New);
                        }
                    });
                });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    for (section, name) in [
                        (Section::Script, "剧本"),
                        (Section::Table, "表格"),
                        (Section::Direction, "演出"),
                        (Section::Characters, "人物"),
                        (Section::Backgrounds, "背景"),
                        (Section::Flags, "变量"),
                        (Section::Audio, "音乐音效"),
                        (Section::Theme, "主题"),
                        (Section::Assets, "素材维护"),
                        (Section::Tests, "分支测试"),
                        (Section::Check, "检查报告"),
                        (Section::Project, "项目设置"),
                    ] {
                        if ui
                            .add(
                                egui::Button::selectable(self.section == section, name)
                                    .frame_when_inactive(false)
                                    .min_size(Vec2::new(60., 34.)),
                            )
                            .clicked()
                        {
                            self.section = section;
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.runtime.is_none(), |ui| {
                        if ui
                            .add_enabled(
                                !self.undo.is_empty(),
                                egui::Button::new("撤销").frame(false),
                            )
                            .on_hover_text("Ctrl+Z")
                            .clicked()
                        {
                            self.history_edit(false);
                        }
                        if ui
                            .add_enabled(
                                !self.redo.is_empty(),
                                egui::Button::new("重做").frame(false),
                            )
                            .on_hover_text("Ctrl+Y")
                            .clicked()
                        {
                            self.history_edit(true);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("使用指南").clicked() {
                            self.help_open = true;
                        }
                        if ui
                            .small_button("A+")
                            .on_hover_text("放大编辑界面")
                            .clicked()
                        {
                            self.ui_scale = (self.ui_scale + 0.1).min(1.3);
                            ctx.set_zoom_factor(self.ui_scale);
                        }
                        if ui
                            .small_button("A−")
                            .on_hover_text("缩小编辑界面")
                            .clicked()
                        {
                            self.ui_scale = (self.ui_scale - 0.1).max(0.8);
                            ctx.set_zoom_factor(self.ui_scale);
                        }
                    });
                });
                self.advanced_toolbar(ui);
            });
        egui::TopBottomPanel::bottom(egui::Id::new(("status", &self.path)))
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(18, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    badge(
                        ui,
                        if self.status_error {
                            "错误"
                        } else if self.dirty() {
                            "编辑中"
                        } else {
                            "已保存"
                        },
                        if self.status_error { BAD } else { GOOD },
                    );
                    ui.add(egui::Label::new(RichText::new(&self.status).size(13.)).truncate())
                        .on_hover_text(&self.status);
                });
                ui.horizontal(|ui| {
                    let folder = self
                        .path
                        .parent()
                        .and_then(|p| p.file_name())
                        .unwrap_or_default()
                        .to_string_lossy();
                    let filename = format!(
                        "{} / {}",
                        folder,
                        self.path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    ui.add(
                        egui::Label::new(RichText::new(&filename).small().color(MUTED)).truncate(),
                    )
                    .on_hover_text(self.path.display().to_string());
                });
            });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        let panel = egui::SidePanel::left(egui::Id::new(("scenes", &self.path)))
            .resizable(true)
            .default_width(self.v5.layout.left_width)
            .min_width(205.)
            .max_width(500.)
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(14),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("剧情目录").size(18.).strong());
                    badge(ui, &self.project.nodes.len().to_string(), MUTED);
                });
                let endings = self
                    .project
                    .nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Ending)
                    .count();
                ui.label(
                    RichText::new(format!(
                        "{} 个人物  /  {} 个结局",
                        self.project.characters.len(),
                        endings
                    ))
                    .small()
                    .color(MUTED),
                );
                self.chapter_filter(ui);
                ui.add_space(7.);
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("搜索剧情…")
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10, 8)),
                );
                ui.add_space(8.);
                let current = self.runtime.as_ref().map(|r| r.current.node.clone());
                if self.filter_revision != self.revision
                    || self.filter_query != self.search
                    || self.filter_chapter != self.advanced.chapter_filter
                {
                    self.filter_rows = self
                        .project
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| {
                            opengalgame::authoring::matches(n, &self.search)
                                && (self.advanced.chapter_filter.is_empty()
                                    || n.chapter == self.advanced.chapter_filter
                                    || (n.chapter.is_empty()
                                        && self.advanced.chapter_filter == "未分章"))
                        })
                        .map(|(i, _)| i)
                        .collect();
                    self.filter_revision = self.revision;
                    self.filter_query = self.search.clone();
                    self.filter_chapter = self.advanced.chapter_filter.clone();
                }
                if self.filter_rows.is_empty() {
                    ui.label(RichText::new("没有匹配的节点").color(MUTED));
                }
                egui::ScrollArea::vertical()
                    .id_salt("node_list")
                    .max_height((ui.available_height() - 106.).max(100.))
                    .auto_shrink([false, false])
                    .show_rows(ui, 62., self.filter_rows.len(), |ui, range| {
                        for row in range {
                            let i = self.filter_rows[row];
                            let node = self.project.nodes[i].clone();
                            let selected = self.selected == i;
                            let color = kind_color(ui, node.kind);
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                &node.title,
                                0.,
                                egui::TextFormat {
                                    font_id: FontId::proportional(15.),
                                    color: INK,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!(
                                    "\n{}  {}{}{}",
                                    node.kind.label(),
                                    node.id,
                                    if self.project.start == node.id {
                                        "  起点"
                                    } else {
                                        ""
                                    },
                                    if current.as_deref() == Some(&node.id) {
                                        "  试玩中"
                                    } else {
                                        ""
                                    }
                                ),
                                0.,
                                egui::TextFormat {
                                    font_id: FontId::proportional(12.),
                                    color: MUTED,
                                    ..Default::default()
                                },
                            );
                            job.wrap.max_width = ui.available_width() - 24.;
                            let response = ui.add_sized(
                                [ui.available_width(), 62.],
                                egui::Button::new(job)
                                    .fill(if selected {
                                        ui.visuals().selection.bg_fill
                                    } else {
                                        ui.visuals().panel_fill
                                    })
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        if selected {
                                            accent(ui)
                                        } else {
                                            Color32::from_rgb(48, 61, 80)
                                        },
                                    ))
                                    .corner_radius(6),
                            );
                            let marker = Rect::from_min_size(
                                response.rect.min + Vec2::new(0., 8.),
                                Vec2::new(3., 46.),
                            );
                            ui.painter().rect_filled(marker, 2, color);
                            if response
                                .clone()
                                .on_hover_text(format!("{}\n{}", node.title, node.id))
                                .clicked()
                            {
                                self.select(i);
                            }
                        }
                    });
                ui.separator();
                ui.label(RichText::new("添加剧情节点").small().color(MUTED));
                ui.add_enabled_ui(self.runtime.is_none(), |ui| {
                    ui.spacing_mut().button_padding = Vec2::new(7., 7.);
                    ui.spacing_mut().item_spacing.x = 6.;
                    ui.horizontal_wrapped(|ui| {
                        for kind in [NodeKind::Dialogue, NodeKind::Choice, NodeKind::Ending] {
                            if ui.button(format!("+ {}", kind.label())).clicked() {
                                let index = (self.selected + 1).min(self.project.nodes.len());
                                let mut node = Node::new(self.project.next_id(), kind, index);
                                if let Some(previous) =
                                    self.project.nodes.get(self.selected).cloned()
                                {
                                    node.background = previous.background;
                                    node.character = previous.character;
                                    node.position =
                                        [previous.position[0] + 40., previous.position[1] + 135.];
                                    if kind == NodeKind::Dialogue {
                                        node.next = if previous.kind == NodeKind::Dialogue {
                                            previous
                                                .next
                                                .clone()
                                                .or(Some(self.project.start.clone()))
                                        } else {
                                            Some(previous.id.clone())
                                        };
                                        if previous.kind == NodeKind::Dialogue {
                                            self.project.nodes[self.selected].next =
                                                Some(node.id.clone());
                                        }
                                    }
                                    if kind == NodeKind::Choice {
                                        let target = previous
                                            .next
                                            .unwrap_or_else(|| self.project.start.clone());
                                        node.choices = vec![
                                            Choice {
                                                text: "第一个选择".into(),
                                                target: target.clone(),
                                                condition: None,
                                                set_flag: None,
                                                when: None,
                                                effects: vec![],
                                            },
                                            Choice {
                                                text: "另一个选择".into(),
                                                target,
                                                condition: None,
                                                set_flag: None,
                                                when: None,
                                                effects: vec![],
                                            },
                                        ];
                                    }
                                }
                                self.project.nodes.insert(index, node);
                                self.select(index);
                                self.section = Section::Script;
                            }
                        }
                    });
                });
            });
        self.v5.layout.left_width = panel.response.rect.width();
    }
    fn inspector(&mut self, ctx: &egui::Context) {
        let panel = egui::SidePanel::right(egui::Id::new(("properties",self.path.clone()))).resizable(true).default_width(self.v5.layout.right_width).min_width(285.).max_width(900.).frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill).inner_margin(18)).show(ctx,|ui| {
            ui.horizontal(|ui|{ui.label(RichText::new("编辑场景").size(18.).strong());if let Some(n)=self.project.nodes.get(self.selected){badge(ui,n.kind.label(),kind_color(ui, n.kind));}});
            ui.add_space(8.);
            if self.project.nodes.is_empty(){ui.label("在左侧添加第一个节点。");return;}
            self.selected=self.selected.min(self.project.nodes.len()-1);
            if self.runtime.is_some(){ui.colored_label(accent(ui),"试玩中。停止试玩后可编辑。");}
            let node_id=self.project.nodes[self.selected].id.clone();
            let inline_errors:Vec<_>=self.issues.iter().filter(|i|i.node.as_deref()==Some(&node_id)&&i.severity==Severity::Error).map(|i|i.message.clone()).collect();
            for message in inline_errors{ui.colored_label(BAD,message);}
            egui::ScrollArea::vertical().id_salt(("node_fields",&node_id)).auto_shrink([false,false]).show(ui,|ui|{ui.add_enabled_ui(self.runtime.is_none(),|ui| {
                let defs:Vec<_>=opengalgame::logic::definitions(&self.project).into_iter().collect();
                let targets=self.node_targets();
                let chars:Vec<_>=self.project.characters.iter().map(|c|(c.id.clone(),c.name.clone())).collect();
                let bgs:Vec<_>=self.project.backgrounds.iter().map(|b|(b.id.clone(),b.name.clone())).collect();
                let flags:Vec<_>=self.project.flags.keys().cloned().collect();
                let node=&mut self.project.nodes[self.selected];
                field_label(ui,"场景标题");ui.add(egui::TextEdit::singleline(&mut node.title).desired_width(f32::INFINITY).margin(8));
                ui.horizontal(|ui|{for kind in [NodeKind::Dialogue,NodeKind::Choice,NodeKind::Ending]{ui.selectable_value(&mut node.kind,kind,kind.label());}});
                section_label(ui,"画面与人物");
                field_label(ui,"背景画面");optional_combo(ui,"bg",&mut node.background,&bgs,"纯色舞台");
                field_label(ui,"发言人");optional_combo(ui,"character",&mut node.character,&chars,"旁白（不显示人物）");
                if node.character.is_some(){
                    ui.horizontal(|ui|{ui.label(RichText::new("立绘位置").size(13.).color(MUTED));for side in [PortraitSide::Left,PortraitSide::Center,PortraitSide::Right]{ui.selectable_value(&mut node.portrait_side,side,side.label());}});
                    ui.horizontal(|ui|{ui.label(RichText::new("大小").size(13.).color(MUTED));ui.add(egui::Slider::new(&mut node.portrait_scale,0.4..=1.6).suffix(" ×").fixed_decimals(1));});
                }
                section_label(ui,"对白内容");
                ui.add(egui::TextEdit::multiline(&mut node.text).desired_width(f32::INFINITY).desired_rows(5).font(FontId::proportional(16.)).margin(10));
                ui.label(RichText::new(format!("{} 字  ·  换行可分段",node.text.chars().count())).small().color(MUTED));
                match node.kind {
                    NodeKind::Dialogue=>{section_label(ui,"剧情衔接");field_label(ui,"读完后前往");optional_combo(ui,"next",&mut node.next,&targets,"请选择下一场景");},
                    NodeKind::Choice=>{
                        section_label(ui,"玩家选项");let mut remove=None;
                        for (index,choice) in node.choices.iter_mut().enumerate(){ui.push_id(index,|ui|{
                            egui::Frame::new().fill(ui.visuals().extreme_bg_color).inner_margin(12).corner_radius(7).stroke(Stroke::new(1.0_f32,Color32::from_rgb(53,66,88))).show(ui,|ui|{
                                ui.horizontal(|ui|{badge(ui,&format!("选项 {}",index+1),accent(ui));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.small_button("移除").clicked(){remove=Some(index);}});});
                                ui.add(egui::TextEdit::multiline(&mut choice.text).desired_width(f32::INFINITY).desired_rows(2).margin(8));
                                field_label(ui,"选择后前往");target_combo(ui,"target",&mut choice.target,&targets);
                                egui::CollapsingHeader::new("条件与变量").id_salt("conditions").default_open(choice.condition.is_some()||choice.set_flag.is_some()).show(ui,|ui|{
                                    flag_control(ui,"仅满足条件时显示",&mut choice.condition,&flags);
                                    flag_control(ui,"选择后改变变量",&mut choice.set_flag,&flags);
                                    logic_ui::choice_logic(ui,choice,&defs);
                                    if flags.is_empty(){ui.label(RichText::new("在「变量」页先创建一个变量。").small().color(MUTED));}
                                });
                            });
                        });}
                        if let Some(index)=remove{node.choices.remove(index);}
                        if ui.add_sized([ui.available_width(),36.],egui::Button::new("+ 添加一个选项")).clicked(){node.choices.push(Choice{text:"新的选择".into(),target:self.project.start.clone(),condition:None,set_flag:None,when:None,effects:vec![]});}
                    },
                    NodeKind::Ending=>{ui.colored_label(GOOD,"这是一个结局，不再继续跳转。");}
                }
                if ui.button("演出 / 配音编排").clicked(){self.section=Section::Direction;}
                if ui.button("展开条件大窗口").clicked(){self.v5.logic_open=true;}
                self.scene_options(ui);
                ui.add_space(10.);
                egui::CollapsingHeader::new("节点标识与操作").id_salt("advanced").show(ui,|ui|{
                    field_label(ui,"标识（更名会同步更新引用）");
                    ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut self.node_id).desired_width((ui.available_width()-72.).max(80.)));if ui.button("更新").clicked(){let old=self.project.nodes[self.selected].id.clone();let new=self.node_id.trim().to_owned();let result=self.project.rename_node(&old,&new).map(|_|"标识及引用已更新");self.message(result);}});
                    ui.horizontal_wrapped(|ui|{
                        if ui.button("设为起点").clicked(){self.project.start=self.project.nodes[self.selected].id.clone();}
                        if ui.button("复制节点").clicked(){let mut copy=self.project.nodes[self.selected].clone();copy.id=self.project.next_id();copy.title.push_str(" 副本");copy.position[1]+=130.;self.project.nodes.insert(self.selected+1,copy);self.select(self.selected+1);}
                        if ui.button("上移").clicked()&&self.selected>0{self.project.nodes.swap(self.selected,self.selected-1);self.select(self.selected-1);}
                        if ui.button("下移").clicked()&&self.selected+1<self.project.nodes.len(){self.project.nodes.swap(self.selected,self.selected+1);self.select(self.selected+1);}
                    });
                    if ui.button(RichText::new("删除此节点…").color(BAD)).clicked(){self.delete_node=true;}
                });
            });});
        });
        self.v5.layout.right_width = panel.response.rect.width();
    }
    fn texture(&mut self, ctx: &egui::Context, file: &str) -> Option<egui::TextureHandle> {
        let base = self.base().to_owned();
        self.images.get(ctx, &base, file, 2048)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, file: &str) -> Option<egui::TextureHandle> {
        let base = self.base().to_owned();
        self.images.get(ctx, &base, file, 512)
    }

    fn scene(&mut self, ui: &mut egui::Ui, size: Vec2, playing: bool) -> Option<Action> {
        let id = if playing {
            self.runtime
                .as_ref()
                .map(|r| r.current.node.clone())
                .unwrap_or_default()
        } else {
            self.project
                .nodes
                .get(self.selected)
                .map(|n| n.id.clone())
                .unwrap_or_default()
        };
        let Some(original) = self.project.node(&id).cloned() else {
            ui.label("没有可显示的节点");
            return None;
        };
        let (node, shade, slide) = self.prepare_scene(&original, playing, ui.ctx());
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8, Color32::from_rgb(25, 42, 58));
        let directed = self.v5.frame.clone();
        let stage_background = directed
            .as_ref()
            .and_then(|f| f.background.clone())
            .or(node.background.clone());
        let background = stage_background
            .as_ref()
            .and_then(|id| self.project.backgrounds.iter().find(|b| &b.id == id))
            .map(|b| b.file.clone());
        if let Some(file) = background
            && let Some(texture) = self.texture(ui.ctx(), &file)
        {
            paint_cover(&painter, rect, &texture);
        }
        painter.rect_filled(rect, 8, Color32::from_black_alpha(28));
        let theme = self.game_theme();
        let compact_player = cfg!(target_os = "android") && playing;
        let dialogue_h = if compact_player {
            let ratio: f32 = if node.kind == NodeKind::Choice {
                0.68
            } else {
                0.52
            };
            size.y * ratio.max(theme.panel_height)
        } else if self.project.production.theme.is_some() {
            size.y * theme.panel_height
        } else if node.kind == NodeKind::Choice {
            (size.y * 0.53).clamp(210., 380.)
        } else {
            (size.y * 0.34).clamp(155., 250.)
        };
        if let Some(frame) = &directed {
            for actor in &frame.actors {
                let speaking = node.character.as_ref() == Some(&actor.actor.character);
                self.paint_directed_actor(ui, &painter, rect, dialogue_h, actor, speaking);
            }
            if frame.shade > 0. {
                painter.rect_filled(
                    rect,
                    8,
                    Color32::from_black_alpha((frame.shade * 255.) as u8),
                );
            }
        } else {
            for actor in &node.actors {
                self.paint_actor(ui, &painter, rect, dialogue_h, actor, slide, false);
            }
            if let Some(character) = &node.character
                && !node.actors.iter().any(|a| &a.character == character)
            {
                self.paint_actor(
                    ui,
                    &painter,
                    rect,
                    dialogue_h,
                    &Actor {
                        character: character.clone(),
                        expression: node.expression.clone(),
                        side: node.portrait_side,
                        scale: node.portrait_scale,
                    },
                    slide,
                    true,
                );
            }
        }
        if !self.v5.sequence_complete {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "演出中…",
                FontId::proportional(18.),
                INK,
            );
            return None;
        }
        painter.text(
            rect.min + Vec2::new(22., 18.),
            egui::Align2::LEFT_TOP,
            format!("{}  {}", node.chapter, node.title),
            FontId::proportional(16.),
            Color32::from_rgb(245, 234, 214),
        );
        let panel = Rect::from_min_max(
            Pos2::new(
                rect.center().x - rect.width() * theme.panel_width / 2.,
                rect.bottom() - dialogue_h,
            ),
            Pos2::new(
                rect.center().x + rect.width() * theme.panel_width / 2.,
                rect.bottom() - 16.,
            ),
        );
        painter.rect_filled(
            panel,
            theme.radius,
            Color32::from_rgba_unmultiplied(
                theme.panel[0],
                theme.panel[1],
                theme.panel[2],
                theme.opacity,
            ),
        );
        let mut action = None;
        let count = node.text.chars().count();
        let visible = if !playing || self.advanced.revealed || self.advanced.settings.cps <= 0. {
            count
        } else {
            (self.dialogue_elapsed(&node) * self.advanced.settings.cps) as usize
        };
        self.advanced.text_incomplete = playing && visible < count;
        if self.advanced.text_incomplete {
            ui.ctx().request_repaint_after(Duration::from_millis(25));
        }
        let shown: String = node.text.chars().take(visible.min(count)).collect();
        let preview_scale = if playing {
            1.0
        } else {
            (size.x / 800.).clamp(0.6, 1.0)
        };
        let dialogue_inset = if compact_player {
            Vec2::new(12., 8.)
        } else {
            Vec2::new(20., 14.)
        } * preview_scale;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("dialogue_overlay")
                .max_rect(panel.shrink2(dialogue_inset)),
        );
        self.game_style(&mut child);
        child.set_clip_rect(panel.shrink2(dialogue_inset));
        child.spacing_mut().item_spacing.y *= preview_scale;
        let speaker = node
            .character
            .as_ref()
            .and_then(|id| self.project.characters.iter().find(|c| &c.id == id));
        child.label(
            RichText::new(speaker.map(|c| c.name.as_str()).unwrap_or(
                if node.kind == NodeKind::Ending {
                    "故事终章"
                } else {
                    "旁白"
                },
            ))
            .font(FontId::new(
                theme.name_size * preview_scale,
                self.v5.font_family.clone(),
            ))
            .background_color(if theme.name_box {
                Color32::from_rgb(theme.choice[0], theme.choice[1], theme.choice[2])
            } else {
                Color32::TRANSPARENT
            })
            .strong()
            .color(
                speaker
                    .filter(|_| self.project.production.theme.is_none())
                    .map(|c| Color32::from_rgb(c.color[0], c.color[1], c.color[2]))
                    .unwrap_or(Color32::from_rgb(
                        theme.name_color[0],
                        theme.name_color[1],
                        theme.name_color[2],
                    )),
            ),
        );
        egui::ScrollArea::vertical()
            .id_salt(("dialogue", playing, &node.id))
            .max_height(child.available_height().max(12.))
            .show(&mut child, |ui| {
                ui.label(
                    RichText::new(shown)
                        .font(FontId::new(
                            self.text_size * preview_scale,
                            self.v5.font_family.clone(),
                        ))
                        .color(Color32::from_rgb(
                            theme.text[0],
                            theme.text[1],
                            theme.text[2],
                        )),
                );
                if node.kind == NodeKind::Choice && !self.advanced.text_incomplete {
                    let choices = if playing {
                        self.runtime
                            .as_ref()
                            .map(|r| r.visible_choices(&self.project))
                            .unwrap_or_default()
                    } else {
                        (0..node.choices.len()).collect()
                    };
                    if choices.is_empty() {
                        ui.colored_label(BAD, "没有满足条件的选项。可回退，或从头试玩。");
                    }
                    for (ordinal, index) in choices.into_iter().enumerate() {
                        if ui
                            .add_sized(
                                [
                                    ui.available_width(),
                                    theme
                                        .button_height
                                        .max(if compact_player { 48. } else { 0. })
                                        * preview_scale,
                                ],
                                egui::Button::new(
                                    RichText::new(format!(
                                        "{}  {}",
                                        ordinal + 1,
                                        node.choices[index].text
                                    ))
                                    .font(FontId::new(
                                        theme.choice_size * preview_scale,
                                        self.v5.font_family.clone(),
                                    ))
                                    .color(Color32::from_rgb(
                                        theme.choice_text[0],
                                        theme.choice_text[1],
                                        theme.choice_text[2],
                                    )),
                                )
                                .fill(Color32::from_rgb(
                                    theme.choice[0],
                                    theme.choice[1],
                                    theme.choice[2],
                                ))
                                .wrap(),
                            )
                            .clicked()
                            && playing
                            && !self.advanced.transitioning
                        {
                            action = Some(Action::Choice(index));
                        }
                    }
                }
            });
        if playing && response.clicked() && !self.advanced.transitioning {
            if self.advanced.text_incomplete {
                self.advanced.revealed = true;
            } else if node.kind == NodeKind::Dialogue {
                action = Some(Action::Next);
            }
        }
        if shade > 0. {
            painter.rect_filled(rect, 8, Color32::from_black_alpha((shade * 255.) as u8));
        }
        action
    }
    fn graph(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("缩放").small().color(MUTED));
            ui.add(egui::Slider::new(&mut self.graph_zoom, 0.4..=1.2).show_value(false));
            ui.label(format!("{}%", (self.graph_zoom * 100.) as i32));
            if ui
                .add_enabled(self.runtime.is_none(), egui::Button::new("自动整理"))
                .clicked()
            {
                arrange_nodes(&mut self.project);
            }
            if ui.button("重置缩放").clicked() {
                self.graph_zoom = 0.8;
            }
        });
        ui.label(
            RichText::new("拖动端口连接节点，点击连线编辑条件。红框：断链；金框：循环；绿线：试玩经过的路线。")
                .small()
                .color(MUTED),
        );
        let zoom = self.graph_zoom;
        egui::ScrollArea::both()
            .id_salt("graph")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let max_x = self
                    .project
                    .nodes
                    .iter()
                    .map(|n| n.position[0] + 300.)
                    .fold(1250., f32::max)
                    * zoom;
                let max_y = self
                    .project
                    .nodes
                    .iter()
                    .map(|n| n.position[1] + (56. + n.choices.len() as f32 * 24.).max(180.) + 40.)
                    .fold(780., f32::max)
                    * zoom;
                let (canvas, _) = ui.allocate_exact_size(Vec2::new(max_x, max_y), Sense::hover());
                let painter = ui.painter_at(canvas);
                let clip = ui.clip_rect().intersect(canvas);
                let min_x = ((clip.left() - canvas.left()).max(0.) as usize / 24) * 24;
                let min_y = ((clip.top() - canvas.top()).max(0.) as usize / 24) * 24;
                let end_x = (clip.right() - canvas.left()).max(0.) as usize;
                let end_y = (clip.bottom() - canvas.top()).max(0.) as usize;
                for y in (min_y..end_y).step_by(24) {
                    for x in (min_x..end_x).step_by(24) {
                        painter.circle_filled(
                            canvas.min + Vec2::new(x as f32, y as f32),
                            1.,
                            Color32::from_rgb(43, 57, 77),
                        );
                    }
                }
                let nodes = self.project.nodes.clone();
                if self.cycle_revision != self.revision {
                    self.cycle_nodes =
                        std::sync::Arc::new(opengalgame::authoring::cycle_nodes(&self.project));
                    self.cycle_revision = self.revision;
                }
                let cycles = self.cycle_nodes.clone();
                let mut route = Vec::new();
                if let Some(r) = &self.runtime {
                    route.extend(r.history.iter().map(|s| s.node.as_str()));
                    route.push(&r.current.node);
                }
                let route_edges: std::collections::HashSet<_> =
                    route.windows(2).map(|pair| (pair[0], pair[1])).collect();
                let clicked = ui.input(|i| {
                    i.pointer
                        .primary_clicked()
                        .then(|| i.pointer.interact_pos())
                        .flatten()
                });
                let clicked_on_node = clicked.is_some_and(|pos| {
                    nodes.iter().any(|n| {
                        Rect::from_min_size(
                            canvas.min + Vec2::new(n.position[0], n.position[1]) * zoom,
                            Vec2::new(222., (56. + n.choices.len() as f32 * 24.).max(105.)) * zoom,
                        )
                        .expand(6.)
                        .contains(pos)
                    })
                });
                for n in &nodes {
                    for (index, target) in n.targets().iter().enumerate() {
                        if let Some(t) = self.project.node(target) {
                            let a = canvas.min
                                + Vec2::new(
                                    n.position[0] + 222.,
                                    n.position[1] + 32. + index as f32 * 24.,
                                ) * zoom;
                            let b =
                                canvas.min + Vec2::new(t.position[0], t.position[1] + 48.) * zoom;
                            let color = if route_edges.contains(&(n.id.as_str(), *target)) {
                                GOOD
                            } else if self
                                .project
                                .nodes
                                .get(self.selected)
                                .is_some_and(|s| s.id == n.id)
                            {
                                accent(ui)
                            } else {
                                Color32::from_rgb(112, 129, 155)
                            };
                            let points = if b.x > a.x {
                                let mid = (a.x + b.x) * 0.5;
                                vec![a, Pos2::new(mid, a.y), Pos2::new(mid, b.y), b]
                            } else {
                                let offset = (38. + index as f32 * 22.) * zoom;
                                vec![
                                    a,
                                    a + Vec2::new(offset, 0.),
                                    Pos2::new(a.x + offset, b.y - 72. * zoom),
                                    Pos2::new(b.x - 25. * zoom, b.y - 72. * zoom),
                                    b - Vec2::new(25. * zoom, 0.),
                                    b,
                                ]
                            };
                            if let Some(pos) = clicked {
                                let close = points.windows(2).any(|segment| {
                                    let delta = segment[1] - segment[0];
                                    let t = ((pos - segment[0]).dot(delta)
                                        / delta.length_sq().max(0.01))
                                    .clamp(0., 1.);
                                    pos.distance(segment[0] + delta * t) < 7.
                                });
                                if close && !clicked_on_node && self.runtime.is_none() {
                                    self.advanced.edge = Some(opengalgame::authoring::Port {
                                        node: n.id.clone(),
                                        choice: (n.kind == NodeKind::Choice).then_some(index),
                                    });
                                }
                            }
                            painter.add(egui::Shape::line(points, Stroke::new(1.6_f32, color)));
                            painter.arrow(
                                b - Vec2::new(9., 0.),
                                Vec2::new(9., 0.),
                                Stroke::new(1.6_f32, color),
                            );
                            if n.kind == NodeKind::Choice {
                                let text: String = n.choices[index].text.chars().take(10).collect();
                                let pos =
                                    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5 - 12. * zoom);
                                let width = text.chars().count() as f32 * 12. * zoom + 16.;
                                let label =
                                    Rect::from_center_size(pos, Vec2::new(width, 24. * zoom));
                                painter.rect_filled(label, 4, ui.visuals().extreme_bg_color);
                                painter.text(
                                    pos,
                                    egui::Align2::CENTER_CENTER,
                                    text,
                                    FontId::proportional((12. * zoom).max(10.)),
                                    color,
                                );
                            }
                        }
                    }
                }
                for (i, node) in nodes.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        canvas.min + Vec2::new(node.position[0], node.position[1]) * zoom,
                        Vec2::new(222., (56. + node.choices.len() as f32 * 24.).max(105.)) * zoom,
                    );
                    if !rect.expand(16.).intersects(ui.clip_rect()) {
                        continue;
                    }
                    let selected = i == self.selected;
                    let active = self
                        .runtime
                        .as_ref()
                        .is_some_and(|r| r.current.node == node.id);
                    let broken = node
                        .targets()
                        .iter()
                        .any(|id| self.project.node(id).is_none())
                        || (node.kind == NodeKind::Dialogue && node.next.is_none())
                        || (node.kind == NodeKind::Choice && node.choices.is_empty());
                    painter.rect_filled(
                        rect,
                        7,
                        if selected {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().panel_fill
                        },
                    );
                    painter.rect_stroke(
                        rect,
                        7,
                        Stroke::new(
                            if selected { 2.0_f32 } else { 1.0_f32 },
                            if broken {
                                BAD
                            } else if active {
                                GOOD
                            } else if cycles.contains(&node.id) {
                                WARN
                            } else if selected {
                                accent(ui)
                            } else {
                                Color32::from_rgb(67, 84, 108)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    painter.rect_filled(
                        Rect::from_min_size(rect.min, Vec2::new(4. * zoom, rect.height())),
                        3,
                        kind_color(ui, node.kind),
                    );
                    painter.text(
                        rect.min + Vec2::new(12., 12.) * zoom,
                        egui::Align2::LEFT_TOP,
                        format!(
                            "{}{}",
                            node.kind.label(),
                            if self.project.start == node.id {
                                " / 起点"
                            } else {
                                ""
                            }
                        ),
                        FontId::proportional((12. * zoom).max(10.)),
                        kind_color(ui, node.kind),
                    );
                    painter.text(
                        rect.min + Vec2::new(12., 37.) * zoom,
                        egui::Align2::LEFT_TOP,
                        node.title.chars().take(12).collect::<String>(),
                        FontId::proportional((16. * zoom).max(11.)),
                        INK,
                    );
                    painter.text(
                        rect.min + Vec2::new(12., 75.) * zoom,
                        egui::Align2::LEFT_TOP,
                        &node.id,
                        FontId::proportional((12. * zoom).max(10.)),
                        MUTED,
                    );
                    // Reserve the left/right edges for ports, so node movement never
                    // steals a connection gesture that starts on a port.
                    let response = ui.interact(
                        rect.shrink2(Vec2::new(14., 0.)),
                        ui.id().with(("node", i)),
                        Sense::click_and_drag(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &node.title)
                    });
                    if response.clicked() {
                        self.select(i);
                    }
                    if response.dragged() && self.runtime.is_none() {
                        let delta = response.drag_delta() / zoom;
                        self.project.nodes[i].position[0] =
                            (self.project.nodes[i].position[0] + delta.x).max(0.);
                        self.project.nodes[i].position[1] =
                            (self.project.nodes[i].position[1] + delta.y).max(0.);
                    }
                }
                self.graph_ports(ui, canvas, &nodes, zoom);
            });
    }
    fn script_center(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.runtime.is_some() {
                    "实时试玩"
                } else {
                    "故事舞台"
                })
                .size(18.)
                .strong(),
            );
            ui.separator();
            if ui.selectable_label(!self.graph, "画面预览").clicked() {
                self.graph = false;
            }
            if ui.selectable_label(self.graph, "剧情分支图").clicked() {
                self.graph = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                badge(
                    ui,
                    if self.runtime.is_some() {
                        "运行中"
                    } else {
                        "实时预览"
                    },
                    if self.runtime.is_some() {
                        GOOD
                    } else {
                        accent(ui)
                    },
                );
            });
        });
        ui.add_space(8.);
        if self.graph {
            if self.runtime.is_some() {
                self.player_controls(ui);
            }
            self.graph(ui);
            return;
        }
        let available = ui.available_size();
        let size = Vec2::new(available.x, (available.y - 90.).max(240.));
        if let Some(action) = self.scene(ui, size, self.runtime.is_some()) {
            self.step(action);
        }
        ui.add_space(12.);
        if self.runtime.is_some() {
            self.player_controls(ui);
            if let Some(runtime) = &self.runtime {
                ui.label(
                    RichText::new(format!(
                        "当前节点：{}  /  已经过 {} 步",
                        runtime.current.node,
                        runtime.history.len()
                    ))
                    .small()
                    .color(MUTED),
                );
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                if tool(ui, Icon::Play, "从当前节点试玩", false).clicked() {
                    self.begin(true);
                }
                ui.label(
                    RichText::new("预览即时更新，F5 从头试玩。")
                        .small()
                        .color(MUTED),
                );
            });
        }
    }
    fn player_controls(&mut self, ui: &mut egui::Ui) {
        let kind = self
            .runtime
            .as_ref()
            .and_then(|r| self.project.node(&r.current.node))
            .map(|n| n.kind);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.runtime.as_ref().is_some_and(|r| !r.history.is_empty()),
                    egui::Button::new("上一句"),
                )
                .clicked()
            {
                self.step(Action::Back);
            }
            if ui.button("存档 / 读取").clicked() {
                self.saves_open = true;
                self.auto = false;
            }
            if ui.button("对白回看").clicked() {
                self.log_open = true;
                self.auto = false;
            }
            ui.add_enabled_ui(kind == Some(NodeKind::Dialogue), |ui| {
                if ui.selectable_label(self.auto, "自动").clicked() {
                    self.auto = !self.auto;
                    self.auto_started = Instant::now();
                }
            });
            if ui
                .add_enabled(
                    kind == Some(NodeKind::Dialogue),
                    egui::Button::new(if cfg!(target_os = "android") {
                        "继续"
                    } else {
                        "继续  空格"
                    })
                    .fill(Color32::from_rgb(72, 67, 115)),
                )
                .clicked()
            {
                self.step(Action::Next);
            }
            if kind == Some(NodeKind::Ending) && ui.button("重新开始").clicked() {
                self.step(Action::Restart);
            }
            if cfg!(target_os = "android") {
                ui.menu_button("更多", |ui| {
                    self.player_menu(ui);
                });
            }
        });
        if self.player_only && !cfg!(target_os = "android") {
            ui.horizontal_wrapped(|ui| {
                self.player_menu(ui);
                ui.label("正文字号");
                if ui
                    .add(egui::Slider::new(&mut self.text_size, 16.0..=30.0))
                    .changed()
                {
                    self.advanced.settings.font = self.text_size;
                    self.persist_profile();
                }
                if ui
                    .button(if self.full_screen {
                        "退出全屏"
                    } else {
                        "全屏"
                    })
                    .clicked()
                {
                    self.full_screen = !self.full_screen;
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.full_screen));
                }
            });
        }
    }
    fn player_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("返回标题").clicked() {
            self.save_slot("auto");
            self.runtime = None;
            self.advanced.title_screen = true;
            self.auto = false;
        }
        if ui.button("设置").clicked() {
            self.advanced.settings_open = true;
            self.auto = false;
        }
        if ui.button("结局收藏").clicked() {
            self.advanced.collection_open = true;
            self.auto = false;
        }
    }
    fn asset_dialog(&mut self) -> Option<String> {
        self.image_dialog(import_asset)
    }
    fn portrait_dialog(&mut self) -> Option<String> {
        self.image_dialog(import_portrait)
    }
    fn image_dialog(
        &mut self,
        import: fn(&Path, &Path) -> anyhow::Result<String>,
    ) -> Option<String> {
        let file = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
            .pick_file()?;
        match import(self.base(), &file) {
            Ok(relative) => Some(relative),
            Err(e) => {
                self.message(Err::<String, _>(e));
                None
            }
        }
    }
    fn characters(&mut self, ui: &mut egui::Ui) {
        self.character_library(ui);
    }
    fn backgrounds(&mut self, ui: &mut egui::Ui) {
        self.background_library(ui);
    }
    fn flags(&mut self, ui: &mut egui::Ui) {
        ui.heading("故事变量");
        self.typed_variables(ui);
        ui.collapsing("旧项目兼容布尔变量", |ui| {
            ui.label("保留原有标识，可在旧条件或复合条件中使用。");
            ui.add_space(14.);
            for (key, value) in &mut self.project.flags {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(key).color(accent(ui)));
                    ui.checkbox(value, "初始值为真");
                });
            }
            ui.separator();
            ui.label("新变量标识（例如 promised）");
            ui.text_edit_singleline(&mut self.new_flag);
            if ui.button("添加变量").clicked() {
                let key = self.new_flag.trim().to_owned();
                if key.is_empty()
                    || self.project.flags.contains_key(&key)
                    || self.project.variables.contains_key(&key)
                {
                    self.message(Err::<String, _>(anyhow::anyhow!("变量标识不能为空或重复")));
                } else {
                    self.project.flags.insert(key, false);
                    self.new_flag.clear();
                }
            }
            if let Some(r) = &self.runtime {
                ui.separator();
                ui.label("当前试玩变量");
                for (key, value) in &r.current.flags {
                    ui.label(format!("{key} = {value}"));
                }
            }
        });
    }
    fn checks(&mut self, ui: &mut egui::Ui) {
        ui.heading("项目检查");
        if ui.button("重新检查全部节点").clicked() {
            self.check();
        }
        if self.issues.is_empty() {
            ui.colored_label(GOOD, "暂无问题。点击检查可验证引用、图片和条件路径。");
        }
        let rows: Vec<_> = self
            .issues
            .iter()
            .map(|i| (i.severity, i.node.clone(), i.message.clone()))
            .collect();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (severity, node, message) in rows {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(14)
                    .corner_radius(5)
                    .show(ui, |ui| {
                        ui.colored_label(
                            if severity == Severity::Error {
                                BAD
                            } else {
                                accent(ui)
                            },
                            format!(
                                "{}：{}",
                                if severity == Severity::Error {
                                    "错误"
                                } else {
                                    "提示"
                                },
                                message
                            ),
                        );
                        if let Some(id) = node
                            && ui.button(format!("定位到 {id}")).clicked()
                            && let Some(i) = self.project.nodes.iter().position(|n| n.id == id)
                        {
                            self.select(i);
                            self.section = Section::Script;
                        }
                    });
            }
        });
    }
    fn project_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("项目设置");
        ui.label("游戏标题");
        ui.add(egui::TextEdit::singleline(&mut self.project.title).desired_width(400.));
        ui.label("作者署名");
        ui.add(egui::TextEdit::singleline(&mut self.project.author).desired_width(400.));
        let targets = self.node_targets();
        ui.label("起始节点");
        target_combo(ui, "project_start", &mut self.project.start, &targets);
        ui.separator();
        ui.label("另存为一个完整项目（含全部引用图片）");
        if ui.button("另存项目到…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("项目", &["json"])
                .set_file_name("project.json")
                .save_file()
        {
            let result = clone_project(&self.project, self.base(), &path);
            match result {
                Ok(p) => {
                    if self.managed {
                        self.workspace_request = Some(WorkspaceRequest::Open(path));
                        self.message(Ok(
                            "已另存版本；保留作品标识，可沿用玩家存档。新作品请用「独立副本」。",
                        ));
                        return;
                    }
                    self.path = path;
                    self.project = p.clone();
                    self.saved = p;
                    self.skip_history = true;
                    self.message(Ok("已另存项目和素材"));
                }
                Err(e) => self.message(Err::<String, _>(e)),
            }
        }
        ui.separator();
        ui.label(RichText::new("OpenGalgame · Rust 原生视觉小说引擎").color(accent(ui)));
        ui.collapsing("存档兼容与节点迁移",|ui|{
            ui.label("调整对白、图片、布局和新增未经过的章节可沿用 0.3 存档。改动已经过节点的类型、跳转、条件或变量时会阻止读取，并说明原因。");
            ui.label("使用节点「更新标识」会自动记录迁移。手动填写可用于删除旧节点后指定替代节点；迁移仍会检查控制流程。");
            ui.horizontal(|ui|{ui.label("旧节点");ui.text_edit_singleline(&mut self.alias_old);});
            ui.horizontal(|ui|{ui.label("新节点");ui.text_edit_singleline(&mut self.alias_new);});
            if ui.button("添加迁移规则").clicked(){let old=self.alias_old.trim().to_owned();let new=self.alias_new.trim().to_owned();let mut policy=self.project.save_compatibility.clone();policy.node_aliases.insert(old.clone(),new);let mut draft=self.project.clone();draft.save_compatibility=policy;if old.is_empty()||draft.node(&old).is_some(){self.message(Err::<String,_>(anyhow::anyhow!("旧标识不能为空，且不能覆盖当前仍存在的节点")));}else{match opengalgame::compatibility::resolve(&draft,&old){Ok(_)=>{self.project.save_compatibility=draft.save_compatibility;self.alias_old.clear();self.alias_new.clear();},Err(e)=>self.message(Err::<String,_>(e))}}}
            let aliases=self.project.save_compatibility.node_aliases.clone();for(old,new)in aliases{ui.horizontal(|ui|{ui.label(format!("{old} → {new}"));if ui.small_button("移除规则").clicked(){self.project.save_compatibility.node_aliases.remove(&old);}});}
            ui.separator();ui.label(format!("已保存 {} 个 0.2 版本基线",self.project.save_compatibility.legacy_baselines.len()));
            ui.label("首次打开 0.2 项目会自动保留基线。其他旧版本请导入当时的原始 project.json；系统只读取该文件，不替换当前内容。");
            if ui.button("导入旧项目兼容基线…").clicked()&&let Some(path)=rfd::FileDialog::new().add_filter("旧项目 JSON",&["json"]).pick_file(){let result=read_project(&path).and_then(|old|opengalgame::compatibility::register_legacy_baseline(&mut self.project,&old)).map(|_|"旧版基线已加入，保存项目后生效");self.message(result);}
        });
        self.presentation_options(ui);
        ui.label("项目保存为可阅读的 JSON；图像与剧情分离。导出目录包含 Game.exe、game.json 和 assets，可直接复制分享。");
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.help_open {
            let mut open = true;
            egui::Window::new("OpenGalgame使用指南").open(&mut open).default_width(590.).resizable(false).show(ctx,|ui| {
                section_label(ui,"从素材到可玩的故事");
                for (title,text) in [
                    ("建立项目","点击「新建」选择父目录，系统会创建独立项目文件夹。"),
                    ("准备人物与背景","进入「人物」「背景」导入 PNG、JPG、WebP。图片会复制到项目，不依赖原路径。"),
                    ("写对白与选择","左侧添加节点，右侧填写标题、画面、发言人和正文。对白指定下一场景，选择设置各选项的目标。"),
                    ("串联故事","通过右侧「节点标识与操作」设置起点。分支图可缩放、拖动和自动整理。"),
                    ("测试并导出","F5 从头试玩。检查没有错误后，可导出 Windows 单文件 EXE、Windows ZIP 或 Android APK。")
                ] {ui.label(RichText::new(title).strong().color(accent(ui)));ui.label(text);ui.add_space(6.);}
                ui.separator();ui.label("Ctrl+S 保存  ·  Ctrl+Z 撤销  ·  Ctrl+Y 重做  ·  F5 试玩");
                ui.label("试玩中：空格继续、左方向键回退、数字键选择、Esc 打开存档。");
            });
            self.help_open = open;
        }
        if let Some(path) = self.export_path.clone() {
            let mut open = true;
            egui::Window::new("游戏导出完成")
                .open(&mut open)
                .collapsible(false)
                .default_width(520.)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&self.project.title).size(24.).color(GOOD));
                    ui.add_space(10.);
                    ui.label(RichText::new(path.display().to_string()).color(accent(ui)));
                    ui.separator();
                    if tool(ui, Icon::Folder, "打开所在文件夹", true).clicked()
                        && let Some(parent) = path.parent()
                        && let Err(e) = std::process::Command::new("explorer.exe")
                            .arg(parent)
                            .spawn()
                    {
                        self.message(Err::<String, _>(e.into()));
                    }
                });
            if !open {
                self.export_path = None;
            }
        }
        if self.pending.is_some() {
            egui::Window::new("保存当前改动？")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("当前项目有未保存的改动。");
                    ui.horizontal(|ui| {
                        if ui.button("保存并继续").clicked()
                            && self.save()
                            && let Some(p) = self.pending.take()
                        {
                            if matches!(p, Pending::Exit) {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                self.perform(p);
                            }
                        }
                        if ui.button("放弃改动并继续").clicked()
                            && let Some(p) = self.pending.take()
                        {
                            let _ = self.checkpoints.dismiss(&self.path, &self.saved);
                            if matches!(p, Pending::Exit) {
                                self.saved = self.project.clone();
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                self.perform(p);
                            }
                        }
                        if ui.button("取消").clicked() {
                            self.pending = None;
                        }
                    });
                });
        }
        if self.delete_node {
            egui::Window::new("删除这个节点？")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("引用此节点的跳转将保留为待修复项，校验会提示位置。可通过撤销恢复。");
                    ui.horizontal(|ui| {
                        if ui.button("确认删除").clicked() {
                            if !self.project.nodes.is_empty() {
                                self.project.nodes.remove(self.selected);
                                self.select(
                                    self.selected
                                        .min(self.project.nodes.len().saturating_sub(1)),
                                );
                            }
                            self.delete_node = false;
                        }
                        if ui.button("取消").clicked() {
                            self.delete_node = false;
                        }
                    });
                });
        }
        if self.saves_open {
            self.save_browser(ctx);
        }
        if self.log_open {
            let mut open = true;
            egui::Window::new("对白回看")
                .open(&mut open)
                .default_size([600., 500.])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(r) = &self.runtime {
                            let ids: Vec<_> = r.history.iter().map(|s| s.node.clone()).collect();
                            for id in ids {
                                if let Some(n) = self.project.node(&id).cloned() {
                                    ui.colored_label(accent(ui), &n.title);
                                    ui.label(&n.text);
                                    if n.direction.voice.is_some()
                                        && ui.button("重播这句配音").clicked()
                                    {
                                        self.replay_voice(&id);
                                    }
                                    ui.separator();
                                }
                            }
                        }
                    });
                });
            self.log_open &= open;
        }
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let possible_edit = !self.player_only
            && self.runtime.is_none()
            && ctx.input(|i| {
                i.pointer.any_down()
                    || i.events.iter().any(|e| {
                        !matches!(
                            e,
                            egui::Event::PointerMoved(_)
                                | egui::Event::PointerGone
                                | egui::Event::MouseMoved(_)
                        )
                    })
            });
        let before = possible_edit.then(|| self.project.clone());
        self.debug_tick(ctx);
        self.production_tick(ctx);
        self.images.poll(ctx);
        self.advanced_tick(ctx);
        self.skip_history = false;
        if ctx.input(|i| i.viewport().close_requested())
            && !self.player_only
            && !self.managed
            && self.dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Exit);
        }
        if !self.player_only
            && !self.external_modal
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
        {
            self.save();
        }
        if !self.player_only && !self.external_modal && self.runtime.is_none() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
                self.history_edit(false);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)) {
                self.history_edit(true);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
                self.begin(false);
            }
        }
        let modal = self.v4.debug.open
            || self.v5.logic_open
            || self.v5.repair.is_some()
            || self.v5.voices.is_some()
            || self.v5.export_preflight
            || self.v4.table.preview.is_some()
            || self.external_modal
            || self.saves_open
            || self.log_open
            || self.pending.is_some()
            || self.delete_node
            || self.advanced.settings_open
            || self.advanced.collection_open
            || self.advanced.recovery.is_some()
            || self.advanced.bulk_open
            || self.advanced.replace_open
            || self.advanced.edge.is_some()
            || self.advanced.delete_asset.is_some();
        if self.runtime.is_some() && !modal && !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Space)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight)
            }) && self
                .runtime
                .as_ref()
                .and_then(|r| self.project.node(&r.current.node))
                .is_some_and(|n| n.kind == NodeKind::Dialogue)
            {
                self.step(Action::Next);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft)) {
                self.step(Action::Back);
            }
            for (index, key) in [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
                egui::Key::Num7,
                egui::Key::Num8,
                egui::Key::Num9,
            ]
            .into_iter()
            .enumerate()
            {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key))
                    && let Some(choice) = self
                        .runtime
                        .as_ref()
                        .and_then(|r| r.visible_choices(&self.project).get(index).copied())
                {
                    self.step(Action::Choice(choice));
                }
            }
        }
        if self.runtime.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.auto = false;
            if self.log_open {
                self.log_open = false;
            } else {
                self.saves_open = !self.saves_open;
            }
        }
        if self.auto && !modal && ctx.input(|i| i.focused) {
            if let Some(node) = self
                .runtime
                .as_ref()
                .and_then(|r| self.project.node(&r.current.node))
            {
                if node.kind != NodeKind::Dialogue {
                    self.auto = false;
                } else {
                    let seconds = (node.text.chars().count() as f32 / 6.).clamp(3., 60.);
                    if self.auto_started.elapsed().as_secs_f32() >= seconds
                        && self.v5.sequence_complete
                        && !(self.advanced.settings.wait_voice
                            && self.advanced.settings.voice > 0.
                            && self.advanced.sound.voice_playing())
                    {
                        self.step(Action::Next);
                    }
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
        } else if self.auto {
            self.auto_started = Instant::now();
        }
        if self.player_only && self.advanced.title_screen {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(ctx.style().visuals.extreme_bg_color))
                .show(ctx, |ui| self.title_ui(ui));
        } else if self.player_only {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.extreme_bg_color)
                        .inner_margin(16),
                )
                .show(ctx, |ui| {
                    if cfg!(target_os = "android") {
                        let size = Vec2::new(
                            ui.available_width(),
                            (ui.available_height() - 64.).max(100.),
                        );
                        if let Some(action) = self.scene(ui, size, true) {
                            self.step(action);
                        }
                        ui.add_space(8.);
                        self.player_controls(ui);
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&self.project.title)
                                .color(accent(ui))
                                .size(18.),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("菜单  Esc").clicked() {
                                self.saves_open = true;
                                self.auto = false;
                            }
                        });
                    });
                    let size = Vec2::new(
                        ui.available_width(),
                        (ui.available_height() - 110.).max(280.),
                    );
                    if let Some(action) = self.scene(ui, size, true) {
                        self.step(action);
                    }
                    ui.add_space(8.);
                    self.player_controls(ui);
                    ui.label(
                        RichText::new(&self.status)
                            .small()
                            .color(if self.status_error { BAD } else { MUTED }),
                    );
                });
        } else {
            self.toolbar(ctx);
            if !self.advanced.focus
                && self.v5.layout.outline
                && !matches!(
                    self.section,
                    Section::Table | Section::Theme | Section::Assets | Section::Tests
                )
            {
                self.sidebar(ctx);
            }
            if self.section == Section::Script
                && self.v5.layout.inspector
                && (!self.advanced.focus || self.runtime.is_some())
            {
                self.inspector(ctx);
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.extreme_bg_color)
                        .inner_margin(18),
                )
                .show(ctx, |ui| {
                    if self.section == Section::Script {
                        if self.advanced.focus && self.runtime.is_none() {
                            self.focus_editor(ui);
                        } else {
                            self.script_center(ui);
                        }
                    } else {
                        ui.add_enabled_ui(
                            self.runtime.is_none() || self.section == Section::Check,
                            |ui| match self.section {
                                Section::Table => self.table_editor(ui),
                                Section::Direction => self.direction_editor(ui),
                                Section::Theme => self.theme_editor(ui),
                                Section::Assets => self.assets_editor(ui),
                                Section::Tests => self.test_editor(ui),
                                Section::Characters => self.characters(ui),
                                Section::Backgrounds => self.backgrounds(ui),
                                Section::Flags => self.flags(ui),
                                Section::Check => self.checks(ui),
                                Section::Project => self.project_settings(ui),
                                Section::Audio => self.audio_library(ui),
                                Section::Script => {}
                            },
                        );
                    }
                });
        }
        self.debugger_window(ctx);
        self.csv_preview(ctx);
        self.production_dialogs(ctx);
        self.dialogs(ctx);
        self.advanced_dialogs(ctx);
        if before.is_some() && self.project.uses_production() {
            self.project.format_version = 3;
        } else if before.is_some() && self.project.uses_typed_logic() {
            self.project.format_version = self.project.format_version.max(2);
        }
        if let Some(before) = before
            && self.project != before
        {
            self.revision += 1;
            self.dirty_flag = self.project != self.saved;
            if !self.dirty_flag {
                let _ = self.checkpoints.dismiss(&self.path, &self.saved);
            }
            if !self.skip_history {
                self.issues.clear();
                if self.last_edit.elapsed() > Duration::from_millis(650) || self.undo.is_empty() {
                    self.undo.push(before);
                    if self.undo.len() > 100 {
                        self.undo.remove(0);
                    }
                }
                self.redo.clear();
                self.last_edit = Instant::now();
            }
        }
    }
}
fn optional_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    items: &[(String, String)],
    empty: &str,
) {
    let selected = value
        .as_ref()
        .map(|id| {
            items
                .iter()
                .find(|(key, _)| key == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| format!("缺失：{id}"))
        })
        .unwrap_or_else(|| empty.into());
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(245.)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, empty);
            for (key, name) in items {
                ui.selectable_value(value, Some(key.clone()), name);
            }
        });
}
fn target_combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[(String, String)]) {
    let selected = items
        .iter()
        .find(|(key, _)| key == value)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| format!("请选择 / 缺失：{value}"));
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(235.)
        .show_ui(ui, |ui| {
            let search_id = ui.id().with("target_search");
            let mut query = ui.data_mut(|d| d.get_temp::<String>(search_id).unwrap_or_default());
            ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("搜索节点名称或标识")
                    .desired_width(235.),
            );
            let filtered: Vec<_> = items
                .iter()
                .filter(|(key, name)| {
                    query.is_empty() || key.contains(&query) || name.contains(&query)
                })
                .collect();
            ui.data_mut(|d| d.insert_temp(search_id, query));
            egui::ScrollArea::vertical().max_height(250.).show_rows(
                ui,
                34.,
                filtered.len(),
                |ui, range| {
                    for i in range {
                        let (key, name) = filtered[i];
                        ui.selectable_value(value, key.clone(), name);
                    }
                },
            );
        });
}
fn flag_control(ui: &mut egui::Ui, label: &str, value: &mut Option<FlagValue>, flags: &[String]) {
    ui.push_id(label, |ui| {
        let mut enabled = value.is_some();
        if ui
            .add_enabled(!flags.is_empty(), egui::Checkbox::new(&mut enabled, label))
            .changed()
        {
            *value = if enabled {
                Some(FlagValue {
                    flag: flags[0].clone(),
                    value: true,
                })
            } else {
                None
            };
        }
        if let Some(f) = value {
            egui::ComboBox::from_id_salt("flag")
                .selected_text(&f.flag)
                .width(150.)
                .show_ui(ui, |ui| {
                    for key in flags {
                        ui.selectable_value(&mut f.flag, key.clone(), key);
                    }
                });
            ui.checkbox(&mut f.value, "值为真（取消则为假）");
        }
    });
}
fn paint_cover(painter: &egui::Painter, rect: Rect, tex: &egui::TextureHandle) {
    let image_ratio = tex.aspect_ratio();
    let rect_ratio = rect.width() / rect.height();
    let mut uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
    if image_ratio > rect_ratio {
        let width = rect_ratio / image_ratio;
        uv.min.x = (1. - width) / 2.;
        uv.max.x = uv.min.x + width;
    } else {
        let height = image_ratio / rect_ratio;
        uv.min.y = (1. - height) / 2.;
        uv.max.y = uv.min.y + height;
    }
    painter.image(tex.id(), rect, uv, Color32::WHITE);
}

#[derive(Clone, Copy)]
enum Icon {
    Play,
    Stop,
    Save,
    Folder,
    Add,
    Check,
    Export,
}
fn tool(ui: &mut egui::Ui, icon: Icon, label: &str, primary: bool) -> egui::Response {
    let fill = if primary {
        accent(ui)
    } else {
        ui.visuals().faint_bg_color
    };
    let color = if primary { text_on(fill) } else { INK };
    let response = ui.add(
        egui::Button::new(RichText::new(format!("     {label}")).color(color))
            .min_size(Vec2::new(0., 36.))
            .corner_radius(6)
            .fill(fill),
    );
    let origin = response.rect.left_center() + Vec2::new(15., 0.);
    let s = Stroke::new(1.7_f32, color);
    let p = ui.painter();
    let line = |points: Vec<Vec2>| {
        p.add(egui::Shape::line(
            points.into_iter().map(|v| origin + v).collect(),
            s,
        ))
    };
    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    origin + Vec2::new(-5., -7.),
                    origin + Vec2::new(7., 0.),
                    origin + Vec2::new(-5., 7.),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(origin, Vec2::splat(12.)), 2, color);
        }
        Icon::Add => {
            line(vec![Vec2::new(-7., 0.), Vec2::new(7., 0.)]);
            line(vec![Vec2::new(0., -7.), Vec2::new(0., 7.)]);
        }
        Icon::Check => {
            line(vec![
                Vec2::new(-7., 0.),
                Vec2::new(-2., 5.),
                Vec2::new(8., -6.),
            ]);
        }
        Icon::Folder => {
            line(vec![
                Vec2::new(-8., 6.),
                Vec2::new(-8., -6.),
                Vec2::new(-2., -6.),
                Vec2::new(0., -3.),
                Vec2::new(8., -3.),
                Vec2::new(8., 6.),
                Vec2::new(-8., 6.),
            ]);
        }
        Icon::Save => {
            line(vec![
                Vec2::new(-7., -7.),
                Vec2::new(4., -7.),
                Vec2::new(7., -4.),
                Vec2::new(7., 7.),
                Vec2::new(-7., 7.),
                Vec2::new(-7., -7.),
            ]);
            line(vec![
                Vec2::new(-3., 7.),
                Vec2::new(-3., 1.),
                Vec2::new(3., 1.),
                Vec2::new(3., 7.),
            ]);
            line(vec![
                Vec2::new(-3., -7.),
                Vec2::new(-3., -2.),
                Vec2::new(3., -2.),
            ]);
        }
        Icon::Export => {
            line(vec![
                Vec2::new(-7., 1.),
                Vec2::new(-7., 7.),
                Vec2::new(7., 7.),
                Vec2::new(7., 1.),
            ]);
            line(vec![Vec2::new(0., 3.), Vec2::new(0., -8.)]);
            line(vec![
                Vec2::new(-5., -3.),
                Vec2::new(0., -8.),
                Vec2::new(5., -3.),
            ]);
        }
    }
    response
}
fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            25,
        ))
        .inner_margin(egui::Margin::symmetric(7, 3))
        .corner_radius(4)
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.).color(color));
        });
}
fn kind_color(ui: &egui::Ui, kind: NodeKind) -> Color32 {
    match kind {
        NodeKind::Dialogue => Color32::from_rgb(155, 187, 231),
        NodeKind::Choice => accent(ui),
        NodeKind::Ending => GOOD,
    }
}
fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.);
    ui.separator();
    ui.label(RichText::new(text).size(14.).strong().color(INK));
}
fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(13.).color(MUTED));
}
