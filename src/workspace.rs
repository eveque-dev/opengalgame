use crate::studio::{self, Studio, WorkspaceRequest};
use eframe::{
    App,
    egui::{self, Color32, RichText, Vec2},
};
use opengalgame::{catalog::*, model::*, project_io::*, recovery};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

const BASE: Color32 = Color32::from_rgb(21, 29, 43);
const PANEL: Color32 = Color32::from_rgb(32, 42, 57);
const MUTED: Color32 = Color32::from_rgb(168, 181, 201);
const ACCENT: Color32 = Color32::from_rgb(169, 156, 241);
const GOOD: Color32 = Color32::from_rgb(128, 200, 179);
const BAD: Color32 = Color32::from_rgb(255, 176, 181);

enum LoadKind {
    Open(bool),
    Info,
}
struct LoadJob {
    path: PathBuf,
    kind: LoadKind,
    ctx: egui::Context,
}
struct Loaded {
    path: PathBuf,
    kind: LoadKind,
    result: Result<Project, String>,
}
#[derive(Clone)]
enum ExportState {
    Queued,
    Running,
    Done,
    Failed(String),
    Cancelled,
}
struct ExportTask {
    title: String,
    destination: PathBuf,
    state: ExportState,
}
struct ExportJob {
    id: usize,
    project: Project,
    base: PathBuf,
    destination: PathBuf,
    ctx: egui::Context,
}
struct ExportResult {
    id: usize,
    result: Result<(), String>,
}
enum PendingClose {
    Tab(usize),
    Window,
}

pub struct Workspace {
    tabs: Vec<Studio>,
    active: Option<usize>,
    home: bool,
    catalog: WorkspaceCatalog,
    loader: Sender<LoadJob>,
    loaded: Receiver<Loaded>,
    loading: HashSet<String>,
    info_errors: HashMap<String, String>,
    exporter: Sender<ExportJob>,
    export_results: Receiver<ExportResult>,
    export_tasks: Vec<ExportTask>,
    export_queue: VecDeque<ExportJob>,
    running_export: Option<usize>,
    jobs_open: bool,
    tasks_open: bool,
    task_project: usize,
    task_filter: Option<usize>,
    task_draft: CreativeTask,
    editing_task: Option<(String, String)>,
    new_open: bool,
    new_title: String,
    new_parent: Option<PathBuf>,
    new_template: ProjectTemplate,
    search: String,
    archived: bool,
    error: String,
    notice: String,
    pending_close: Option<PendingClose>,
    quit_after_exports: bool,
    restore_active: Option<PathBuf>,
    restoring: bool,
    templates: Vec<PathBuf>,
    catalog_changed: bool,
    last_catalog_write: Instant,
    saved_generations: HashMap<String, u64>,
    allow_exit: bool,
    session_changed: bool,
    fork_source: Option<usize>,
}
impl Workspace {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        directory: PathBuf,
    ) -> Self {
        studio::configure(&cc.egui_ctx);
        let (loader, jobs) = mpsc::channel::<LoadJob>();
        let (results, loaded) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(job) = jobs.recv() {
                let result = read_project(&job.path).map_err(|e| format!("{e:#}"));
                if results
                    .send(Loaded {
                        path: job.path,
                        kind: job.kind,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                job.ctx.request_repaint();
            }
        });
        let (exporter, exports) = mpsc::channel::<ExportJob>();
        let (results, export_results) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(job) = exports.recv() {
                let result = std::env::current_exe()
                    .map_err(anyhow::Error::from)
                    .and_then(|exe| export_game(&job.project, &job.base, &exe, &job.destination))
                    .map(|_| ())
                    .map_err(|e| format!("{e:#}"));
                if results.send(ExportResult { id: job.id, result }).is_err() {
                    break;
                }
                job.ctx.request_repaint();
            }
        });
        let mut templates: Vec<PathBuf> = vec![];
        for base in [directory, PathBuf::from(env!("CARGO_MANIFEST_DIR"))] {
            for name in ["first-light", "long-form", "harbor-signals"] {
                let path = base.join(format!("examples/{name}/project.json"));
                if path.is_file() && !templates.iter().any(|p| path_key(p) == path_key(&path)) {
                    templates.push(path);
                }
            }
        }
        let mut app = Self {
            tabs: vec![],
            active: None,
            home: true,
            catalog: WorkspaceCatalog::load(),
            loader,
            loaded,
            loading: HashSet::new(),
            info_errors: HashMap::new(),
            exporter,
            export_results,
            export_tasks: vec![],
            export_queue: VecDeque::new(),
            running_export: None,
            jobs_open: false,
            tasks_open: false,
            task_project: 0,
            task_filter: None,
            task_draft: CreativeTask {
                id: String::new(),
                title: String::new(),
                notes: String::new(),
                status: TaskStatus::Todo,
            },
            editing_task: None,
            new_open: false,
            new_title: String::new(),
            new_parent: None,
            new_template: ProjectTemplate::Blank,
            search: String::new(),
            archived: false,
            error: String::new(),
            notice: String::new(),
            pending_close: None,
            quit_after_exports: false,
            restore_active: None,
            restoring: false,
            templates,
            catalog_changed: false,
            last_catalog_write: Instant::now(),
            allow_exit: false,
            session_changed: false,
            fork_source: None,
            saved_generations: HashMap::new(),
        };
        for card in &app.catalog.projects {
            let _ = app.loader.send(LoadJob {
                path: card.path.clone(),
                kind: LoadKind::Info,
                ctx: cc.egui_ctx.clone(),
            });
        }
        if let Some(path) = initial {
            app.open(path, true, &cc.egui_ctx);
        }
        app
    }
    fn save_session(&mut self) {
        self.session_changed = true;
        self.catalog.open_paths = self.tabs.iter().map(|t| t.path().to_owned()).collect();
        self.catalog.active_path = self
            .active
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.path().to_owned());
        self.catalog_changed = true;
    }
    fn open(&mut self, path: PathBuf, activate: bool, ctx: &egui::Context) {
        let key = path_key(&path);
        if let Some(i) = self.tabs.iter().position(|t| path_key(t.path()) == key) {
            if activate {
                self.activate(i);
            }
            return;
        }
        if self.loading.insert(key)
            && self
                .loader
                .send(LoadJob {
                    path,
                    kind: LoadKind::Open(activate),
                    ctx: ctx.clone(),
                })
                .is_err()
        {
            self.error = "项目读取服务已停止，请重新打开应用。".into();
        }
    }
    fn activate(&mut self, index: usize) {
        if let Some(i) = self.active.filter(|&i| i != index)
            && let Some(tab) = self.tabs.get_mut(i)
        {
            tab.pause();
        }
        self.active = Some(index);
        self.home = false;
        self.save_session();
    }
    fn show_home(&mut self) {
        if let Some(i) = self.active
            && let Some(tab) = self.tabs.get_mut(i)
        {
            tab.pause();
        }
        self.home = true;
    }
    fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("打开视觉小说项目")
            .add_filter("项目 JSON", &["json"])
            .pick_file()
        {
            self.open(path, true, ctx);
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(loaded) = self.loaded.try_recv() {
            let key = path_key(&loaded.path);
            match loaded.kind {
                LoadKind::Open(activate) => {
                    self.loading.remove(&key);
                    match loaded.result {
                        Ok(p) => {
                            self.catalog.register(&loaded.path, &p);
                            self.info_errors.remove(&key);
                            let mut tab =
                                Studio::create(p, recovery::normalized(&loaded.path), false);
                            tab.managed = true;
                            self.tabs.push(tab);
                            self.catalog_changed = true;
                            if activate {
                                self.activate(self.tabs.len() - 1);
                            } else {
                                self.save_session();
                            }
                        }
                        Err(e) => {
                            self.info_errors.insert(key, e.clone());
                            self.error = format!("无法打开 {}：{e}", loaded.path.display());
                            self.home = true;
                        }
                    }
                }
                LoadKind::Info => match loaded.result {
                    Ok(p) => {
                        if let Some(card) = self
                            .catalog
                            .projects
                            .iter_mut()
                            .find(|c| path_key(&c.path) == key)
                        {
                            card.refresh(&p);
                            self.catalog_changed = true;
                        }
                    }
                    Err(e) => {
                        self.info_errors.insert(key, e);
                    }
                },
            }
        }
        if self.restoring && self.loading.is_empty() {
            self.restoring = false;
            if let Some(path) = self.restore_active.take()
                && let Some(i) = self
                    .tabs
                    .iter()
                    .position(|t| path_key(t.path()) == path_key(&path))
            {
                self.activate(i);
            } else if !self.tabs.is_empty() {
                self.activate(0);
            }
        }
        while let Ok(done) = self.export_results.try_recv() {
            if let Some(task) = self.export_tasks.get_mut(done.id) {
                task.state = match done.result {
                    Ok(()) => {
                        self.notice = format!("《{}》已导出。可在后台任务中打开目录。", task.title);
                        ExportState::Done
                    }
                    Err(e) => ExportState::Failed(e),
                };
            }
            self.running_export = None;
        }
        if self.running_export.is_none()
            && let Some(job) = self.export_queue.pop_front()
        {
            let id = job.id;
            self.export_tasks[id].state = ExportState::Running;
            self.running_export = Some(id);
            if self.exporter.send(job).is_err() {
                self.export_tasks[id].state = ExportState::Failed("导出服务已停止".into());
                self.running_export = None;
            }
        }
        for tab in &mut self.tabs {
            tab.maintain_recovery(ctx);
            let key = path_key(tab.path());
            if self.saved_generations.get(&key).copied().unwrap_or(0) != tab.saved_generation {
                self.catalog.register(tab.path(), tab.project());
                self.saved_generations.insert(key, tab.saved_generation);
                self.catalog_changed = true;
            }
        }
        if self.catalog_changed && self.last_catalog_write.elapsed() > Duration::from_millis(500) {
            match self.catalog.save() {
                Ok(()) => {
                    self.catalog_changed = false;
                    self.last_catalog_write = Instant::now();
                }
                Err(e) => self.error = format!("工作区记录保存失败：{e}"),
            }
        }
        if !self.loading.is_empty() || self.running_export.is_some() || self.catalog_changed {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.quit_after_exports && self.running_export.is_none() && self.export_queue.is_empty()
        {
            self.quit_after_exports = false;
            self.save_session();
            let _ = self.catalog.save();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn queue_export(
        &mut self,
        project: Project,
        base: PathBuf,
        destination: PathBuf,
        ctx: &egui::Context,
    ) {
        let id = self.export_tasks.len();
        self.export_tasks.push(ExportTask {
            title: project.title.clone(),
            destination: destination.clone(),
            state: ExportState::Queued,
        });
        self.export_queue.push_back(ExportJob {
            id,
            project,
            base,
            destination,
            ctx: ctx.clone(),
        });
        self.jobs_open = true;
    }
    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs[index].discard_recovery();
        self.tabs.remove(index);
        self.active = if self.tabs.is_empty() {
            self.home = true;
            None
        } else {
            Some(
                self.active
                    .unwrap_or(0)
                    .saturating_sub(usize::from(self.active.unwrap_or(0) >= index))
                    .min(self.tabs.len() - 1),
            )
        };
        self.editing_task = None;
        self.task_draft.title.clear();
        self.task_draft.notes.clear();
        self.task_filter = None;
        self.save_session();
    }
    fn save_all(&mut self) -> bool {
        let mut ok = true;
        for tab in &mut self.tabs {
            if tab.dirty() && !tab.save() {
                ok = false;
                self.error = format!("《{}》保存失败，请回到项目查看原因。", tab.project().title);
            }
        }
        ok
    }
    fn tabs_bar(&mut self, ctx: &egui::Context) {
        let mut activate = None;
        let mut close = None;
        let mut home = false;
        egui::TopBottomPanel::top("workspace_tabs")
            .frame(egui::Frame::new().fill(BASE).inner_margin(10))
            .show(ctx, |ui| {
                if self.new_open || self.pending_close.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.home, "项目中心").clicked() {
                        home = true;
                    }
                    ui.separator();
                    egui::ScrollArea::horizontal()
                        .id_salt("project_tabs")
                        .max_width((ui.available_width() - 330.).max(180.))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    ui.push_id(path_key(tab.path()), |ui| {
                                        egui::Frame::new()
                                            .fill(if self.active == Some(i) && !self.home {
                                                Color32::from_rgb(62, 54, 91)
                                            } else {
                                                PANEL
                                            })
                                            .corner_radius(5)
                                            .inner_margin(3)
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    let name = format!(
                                                        "{}{}",
                                                        if tab.dirty() { "● " } else { "" },
                                                        tab.project().title
                                                    );
                                                    if ui
                                                        .add(
                                                            egui::Button::selectable(
                                                                self.active == Some(i)
                                                                    && !self.home,
                                                                RichText::new(name).size(14.),
                                                            )
                                                            .min_size(Vec2::new(80., 34.)),
                                                        )
                                                        .on_hover_text(
                                                            tab.path().display().to_string(),
                                                        )
                                                        .clicked()
                                                    {
                                                        activate = Some(i);
                                                    }
                                                    if ui
                                                        .small_button("×")
                                                        .on_hover_text("关闭项目标签")
                                                        .clicked()
                                                    {
                                                        close = Some(i);
                                                    }
                                                });
                                            });
                                    });
                                }
                            });
                        });
                    if ui.button("＋ 新项目").clicked() {
                        self.new_open = true;
                        self.fork_source = None;
                    }
                    if let Some(i) = self.active
                        && ui
                            .button("独立副本")
                            .on_hover_text("复制当前内容和素材，创建新的作品标识与存档空间")
                            .clicked()
                    {
                        self.fork_source = Some(i);
                        self.new_title = format!("{} · 副本", self.tabs[i].project().title);
                        self.new_open = true;
                    }
                    if ui.button("创作待办").clicked() {
                        self.tasks_open = true;
                        self.task_project = self.active.unwrap_or(0);
                    }
                    let busy = self.running_export.is_some() || !self.export_queue.is_empty();
                    if ui
                        .button(if busy {
                            "后台任务 · 运行中"
                        } else {
                            "后台任务"
                        })
                        .clicked()
                    {
                        self.jobs_open = true;
                    }
                });
            });
        if home {
            self.show_home();
        }
        if let Some(i) = activate {
            self.activate(i);
        }
        if let Some(i) = close {
            if self.tabs[i].dirty() {
                self.pending_close = Some(PendingClose::Tab(i));
            } else {
                self.close_tab(i);
            }
        }
    }
    fn center(&mut self, ctx: &egui::Context) {
        let mut open = None;
        let mut sample = None;
        let mut refresh = false;
        let mut restore = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BASE).inner_margin(28)).show(ctx,|ui|{
            ui.horizontal(|ui|{ui.vertical(|ui|{ui.label(RichText::new("OpenGalgame").size(34.).strong());ui.label(RichText::new("项目中心 · 让每一个故事都有自己的工作台").size(16.).color(MUTED));});ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add_sized([130.,44.],egui::Button::new("打开已有项目")).clicked(){open=Some(None);}
if ui.add_sized([130.,44.],egui::Button::new("新建视觉小说").fill(Color32::from_rgb(79,66,126))).clicked(){self.new_open=true;self.fork_source=None;}});});
            ui.add_space(18.);ui.horizontal(|ui|{ui.label(format!("{} 个作品",self.catalog.projects.len()));ui.separator();ui.label(format!("{} 个项目已打开",self.tabs.len()));ui.separator();let total:usize=self.tabs.iter().map(|t|t.project().tasks.len()).sum();let done:usize=self.tabs.iter().map(|t|t.project().tasks.iter().filter(|v|v.status==TaskStatus::Done).count()).sum();ui.label(format!("创作待办 {done} / {total}"));ui.separator();ui.colored_label(ACCENT,"0.4 · 剧本与调试");});
            ui.add_space(14.);
            if self.tabs.is_empty()&&!self.catalog.open_paths.is_empty()&& ui.button(format!("恢复上次工作区 · {} 个项目",self.catalog.open_paths.len())).clicked(){restore=true;}
            ui.horizontal_wrapped(|ui|{ui.label("快速体验");for path in &self.templates {let name=path.to_string_lossy();if ui.button(if name.contains("harbor-signals"){"灯塔来信 · 变量与调试"}else if name.contains("long-form"){"三封来信 · 42 节点"}else{"入门样例 · 8 节点"}).clicked(){sample=Some(path.clone());}}
if self.templates.is_empty(){ui.label(RichText::new("单文件版也可直接新建空白或分支项目，无需下载素材。").small().color(MUTED));}});
            ui.add_space(12.);ui.separator();ui.horizontal(|ui|{ui.label(RichText::new("我的作品").size(22.).strong());ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("搜索作品名称或目录").desired_width(260.));ui.checkbox(&mut self.archived,"查看归档");if ui.button("刷新信息").clicked(){refresh=true;}
if ui.button("保存全部项目").clicked()&& self.save_all(){self.notice="所有未保存项目已写入磁盘。".into();}});
            if !self.loading.is_empty(){ui.horizontal(|ui|{ui.spinner();ui.label(format!("正在打开 {} 个项目…",self.loading.len()));});}
            if !self.error.is_empty(){ui.colored_label(BAD,&self.error);}else if !self.notice.is_empty(){ui.colored_label(GOOD,&self.notice);}
            ui.add_space(12.);
            let mut indices:Vec<_>=(0..self.catalog.projects.len()).filter(|&i|{let c=&self.catalog.projects[i];(c.stage==ProjectStage::Archived)==self.archived&&(self.search.is_empty()||c.title.contains(&self.search)||c.path.to_string_lossy().contains(&self.search))}).collect();
            indices.sort_by_key(|&i|(!self.catalog.projects[i].pinned,std::cmp::Reverse(self.catalog.projects[i].last_opened)));
            if indices.is_empty(){egui::Frame::new().fill(PANEL).inner_margin(30).corner_radius(8).show(ui,|ui|{ui.label(RichText::new("为下一部作品，留一个位置。").size(25.));ui.label("新建项目，或打开你已有的 project.json。每个作品的待办、存档与编辑状态独立保存。");});}
            egui::ScrollArea::vertical().id_salt("project_cards").show(ui,|ui|{
                for i in indices {
                    let card=self.catalog.projects[i].clone();let key=path_key(&card.path);let tab=self.tabs.iter().position(|t|path_key(t.path())==key);let missing=!card.path.is_file();
                    egui::Frame::new().fill(PANEL).corner_radius(8).inner_margin(18).show(ui,|ui|{
                        ui.horizontal(|ui|{
                            ui.vertical(|ui|{let title=tab.map(|j|self.tabs[j].project().title.as_str()).unwrap_or(&card.title);ui.label(RichText::new(title).size(21.).strong());let parent=card.path.parent().and_then(|p|p.file_name()).unwrap_or_default().to_string_lossy();ui.label(RichText::new(format!("{parent} / project.json")).small().color(MUTED)).on_hover_text(card.path.display().to_string());let(tdone,ttotal)=tab.map(|j|{let tasks=&self.tabs[j].project().tasks;(tasks.iter().filter(|t|t.status==TaskStatus::Done).count(),tasks.len())}).unwrap_or((card.tasks_done,card.tasks_total));ui.label(format!("{} 节点  ·  {} 字（上次读取）  ·  待办 {tdone}/{ttotal}",tab.map(|j|self.tabs[j].project().nodes.len()).unwrap_or(card.nodes),card.words));if let Some(j)=tab&&self.tabs[j].dirty(){ui.colored_label(ACCENT,"已打开 · 有未保存内容");}
if missing{ui.colored_label(BAD,"目录已移动或项目文件不存在");}else if let Some(error)=self.info_errors.get(&key){ui.colored_label(BAD,error);}});
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add_enabled(!missing,egui::Button::new(if tab.is_some(){"回到编辑"}else{"打开项目"})).clicked(){open=Some(Some(card.path.clone()));}
if ui.selectable_label(card.pinned,"置顶").clicked(){self.catalog.projects[i].pinned= !card.pinned;self.catalog_changed=true;}egui::ComboBox::from_id_salt(("stage",i)).selected_text(card.stage.label()).show_ui(ui,|ui|{for stage in [ProjectStage::Draft,ProjectStage::Production,ProjectStage::Complete,ProjectStage::Archived]{let label=stage.label().to_owned();if ui.selectable_value(&mut self.catalog.projects[i].stage,stage,label).changed(){self.catalog_changed=true;}}});});
                        });
                    });ui.add_space(12.);
                }
            });
        });
        if let Some(path) = open {
            if let Some(path) = path {
                self.open(path, true, ctx);
            } else {
                self.open_dialog(ctx);
            }
        }
        if let Some(path) = sample {
            self.open(path, true, ctx);
        }
        if refresh {
            for c in &self.catalog.projects {
                let _ = self.loader.send(LoadJob {
                    path: c.path.clone(),
                    kind: LoadKind::Info,
                    ctx: ctx.clone(),
                });
            }
            self.notice = "正在后台刷新作品信息。".into();
        }
        if restore {
            self.restore_active = self.catalog.active_path.clone();
            let paths = self.catalog.open_paths.clone();
            self.restoring = true;
            for path in paths {
                self.open(path, false, ctx);
            }
        }
    }
    fn new_project_window(&mut self, ctx: &egui::Context) {
        if !self.new_open {
            return;
        }
        let mut open = true;
        let mut create = false;
        egui::Window::new("新建视觉小说")
            .open(&mut open)
            .default_width(530.)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("作品名称");
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_title)
                        .desired_width(f32::INFINITY)
                        .hint_text("例如：海风与你"),
                );
                ui.add_space(8.);
                if self.fork_source.is_none() {
                    ui.horizontal(|ui| {
                        ui.label("项目模板");
                        ui.selectable_value(
                            &mut self.new_template,
                            ProjectTemplate::Blank,
                            "空白故事",
                        );
                        ui.selectable_value(
                            &mut self.new_template,
                            ProjectTemplate::Branching,
                            "双结局分支",
                        );
                    });
                } else {
                    ui.label("独立副本将保留当前未保存内容和待办，并复制素材；原作品不会改变。");
                }
                ui.label(
                    RichText::new("每个项目都会创建独立目录与标识，玩家存档不会串到其他作品。")
                        .small()
                        .color(MUTED),
                );
                if ui.button("选择保存位置").clicked() {
                    self.new_parent = rfd::FileDialog::new()
                        .set_title("选择项目父目录")
                        .pick_folder();
                }
                if let Some(parent) = &self.new_parent {
                    ui.label(parent.display().to_string());
                }
                if !self.error.is_empty() {
                    ui.colored_label(BAD, &self.error);
                }
                if ui
                    .add_enabled(
                        !self.new_title.trim().is_empty() && self.new_parent.is_some(),
                        egui::Button::new("创建并打开").fill(Color32::from_rgb(79, 66, 126)),
                    )
                    .clicked()
                {
                    create = true;
                }
            });
        self.new_open &= open;
        if create && let Some(parent) = &self.new_parent {
            match if let Some(i) = self.fork_source {
                fork_project(
                    self.tabs[i].project(),
                    self.tabs[i].path().parent().unwrap(),
                    parent,
                    &self.new_title,
                )
            } else {
                create_project(parent, &self.new_title, self.new_template)
            } {
                Ok(path) => {
                    self.new_open = false;
                    self.new_title.clear();
                    self.error.clear();
                    self.open(path, true, ctx);
                }
                Err(e) => self.error = format!("{e:#}"),
            }
        }
    }
    fn task_window(&mut self, ctx: &egui::Context) {
        if !self.tasks_open {
            return;
        }
        let mut open = true;
        let mut status_change = None;
        let mut edit = None;
        let mut apply = false;
        egui::Window::new("跨项目创作待办")
            .open(&mut open)
            .default_size([1040., 700.])
            .show(ctx, |ui| {
                if self.tabs.is_empty() {
                    ui.label("先打开或新建一个项目，再安排创作待办。");
                    return;
                }
                self.task_project = self.task_project.min(self.tabs.len() - 1);
                if let Some((key, _)) = &self.editing_task {
                    if let Some(i) = self.tabs.iter().position(|t| path_key(t.path()) == *key) {
                        self.task_project = i;
                    } else {
                        self.editing_task = None;
                        self.task_draft.title.clear();
                        self.task_draft.notes.clear();
                    }
                }
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("task_filter")
                        .selected_text(
                            self.task_filter
                                .and_then(|i| self.tabs.get(i))
                                .map(|t| t.project().title.as_str())
                                .unwrap_or("所有已打开作品"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.task_filter, None, "所有已打开作品");
                            for (i, t) in self.tabs.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.task_filter,
                                    Some(i),
                                    &t.project().title,
                                );
                            }
                        });
                    ui.label("待办随项目保存；切换作品不会丢失。");
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(350.)
                    .show(ui, |ui| {
                        ui.columns(3, |columns| {
                            for (column, status, label) in [
                                (0, TaskStatus::Todo, "待处理"),
                                (1, TaskStatus::Doing, "进行中"),
                                (2, TaskStatus::Done, "已完成"),
                            ] {
                                let ui = &mut columns[column];
                                ui.label(RichText::new(label).size(20.).strong());
                                for (i, tab) in self.tabs.iter().enumerate() {
                                    if self.task_filter.is_some_and(|j| j != i) {
                                        continue;
                                    }
                                    for task in
                                        tab.project().tasks.iter().filter(|t| t.status == status)
                                    {
                                        egui::Frame::new()
                                            .fill(BASE)
                                            .corner_radius(6)
                                            .inner_margin(12)
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(&tab.project().title)
                                                        .small()
                                                        .color(ACCENT),
                                                );
                                                ui.label(RichText::new(&task.title).strong());
                                                if !task.notes.is_empty() {
                                                    ui.label(&task.notes);
                                                }
                                                ui.add_enabled_ui(!tab.is_playing(), |ui| {
                                                    ui.horizontal_wrapped(|ui| {
                                                        if ui.small_button("编辑").clicked() {
                                                            edit = Some((
                                                                path_key(tab.path()),
                                                                task.clone(),
                                                            ));
                                                        }
                                                        for (value, label) in [
                                                            (TaskStatus::Todo, "待处理"),
                                                            (TaskStatus::Doing, "开始"),
                                                            (TaskStatus::Done, "完成"),
                                                        ] {
                                                            if value != status
                                                                && ui.small_button(label).clicked()
                                                            {
                                                                status_change = Some((
                                                                    i,
                                                                    task.id.clone(),
                                                                    value,
                                                                ));
                                                            }
                                                        }
                                                    });
                                                });
                                            });
                                        ui.add_space(8.);
                                    }
                                }
                            }
                        });
                    });
                ui.separator();
                ui.label(
                    RichText::new(if self.editing_task.is_some() {
                        "编辑任务"
                    } else {
                        "添加任务"
                    })
                    .size(19.)
                    .strong(),
                );
                ui.add_enabled_ui(self.editing_task.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("task_owner")
                        .selected_text(&self.tabs[self.task_project].project().title)
                        .show_ui(ui, |ui| {
                            for (i, tab) in self.tabs.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.task_project,
                                    i,
                                    &tab.project().title,
                                );
                            }
                        });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.task_draft.title)
                        .hint_text("例如：完成第二章分支、替换角色表情")
                        .desired_width(f32::INFINITY),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.task_draft.notes)
                        .hint_text("备注与验收标准")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.task_draft.title.trim().is_empty()
                                && !self.tabs[self.task_project].is_playing(),
                            egui::Button::new("应用任务（Ctrl+S 保存项目）"),
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                    if self.editing_task.is_some() && ui.button("取消编辑").clicked() {
                        self.editing_task = None;
                        self.task_draft.title.clear();
                        self.task_draft.notes.clear();
                    }
                    if ui.button("保存全部项目").clicked() {
                        self.save_all();
                    }
                });
            });
        self.tasks_open &= open;
        if let Some((i, id, status)) = status_change {
            self.tabs[i].edit_external(|p| {
                if let Some(task) = p.tasks.iter_mut().find(|t| t.id == id) {
                    task.status = status;
                }
            });
        }
        if let Some((key, task)) = edit
            && let Some(i) = self.tabs.iter().position(|t| path_key(t.path()) == key)
        {
            self.task_project = i;
            self.editing_task = Some((key, task.id.clone()));
            self.task_draft = task;
        }
        if apply && !self.tabs[self.task_project].is_playing() {
            let target = self
                .editing_task
                .as_ref()
                .and_then(|(key, _)| self.tabs.iter().position(|t| path_key(t.path()) == *key))
                .unwrap_or(self.task_project);
            let mut task = self.task_draft.clone();
            task.title = task.title.trim().to_owned();
            let existing = self.editing_task.take().map(|(_, id)| id);
            self.tabs[target].edit_external(|p| {
                if let Some(id) = existing {
                    if let Some(old) = p.tasks.iter_mut().find(|t| t.id == id) {
                        task.id = id;
                        *old = task;
                    }
                } else {
                    task.id = unique_id();
                    p.tasks.push(task);
                }
            });
            self.task_draft.title.clear();
            self.task_draft.notes.clear();
            self.task_draft.status = TaskStatus::Todo;
        }
    }
    fn job_window(&mut self, ctx: &egui::Context) {
        if !self.jobs_open {
            return;
        }
        let mut open = true;
        let mut cancel = None;
        egui::Window::new("后台任务")
            .open(&mut open)
            .default_size([650., 450.])
            .show(ctx, |ui| {
                ui.label("导出在后台进行，可继续编辑其他作品。每次导出使用提交任务时的内容快照。");
                if self.export_tasks.is_empty() {
                    ui.label("还没有导出任务。在任意项目中选择「导出游戏」即可加入队列。");
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, task) in self.export_tasks.iter().enumerate().rev() {
                        egui::Frame::new()
                            .fill(BASE)
                            .inner_margin(12)
                            .corner_radius(6)
                            .show(ui, |ui| {
                                ui.label(RichText::new(&task.title).strong());
                                match &task.state {
                                    ExportState::Queued => {
                                        ui.horizontal(|ui| {
                                            ui.label("等待导出");
                                            if ui.button("取消等待").clicked() {
                                                cancel = Some(i);
                                            }
                                        });
                                    }
                                    ExportState::Running => {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("正在检查并导出…");
                                        });
                                    }
                                    ExportState::Done => {
                                        ui.colored_label(GOOD, "导出完成");
                                        if ui.button("打开输出目录").clicked() {
                                            let _ = std::process::Command::new("explorer.exe")
                                                .arg(&task.destination)
                                                .spawn();
                                        }
                                    }
                                    ExportState::Failed(e) => {
                                        ui.colored_label(BAD, e);
                                    }
                                    ExportState::Cancelled => {
                                        ui.label("已取消等待");
                                    }
                                }
                                ui.label(
                                    RichText::new(task.destination.display().to_string())
                                        .small()
                                        .color(MUTED),
                                );
                            });
                    }
                });
            });
        self.jobs_open &= open;
        if let Some(id) = cancel {
            self.export_queue.retain(|j| j.id != id);
            self.export_tasks[id].state = ExportState::Cancelled;
        }
    }
    fn close_window(&mut self, ctx: &egui::Context) {
        if self.pending_close.is_none() {
            return;
        }
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("workspace_close")).show(ctx, |ui| {
            ui.set_width(530.);
            ui.heading("保留这次创作");
            let window = matches!(self.pending_close, Some(PendingClose::Window));
            let ids: Vec<_> = if window {
                (0..self.tabs.len()).collect()
            } else if let Some(PendingClose::Tab(i)) = self.pending_close {
                vec![i]
            } else {
                vec![]
            };
            for i in ids {
                if let Some(t) = self.tabs.get(i)
                    && t.dirty()
                {
                    ui.label(format!("未保存：{}", t.project().title));
                }
            }
            let busy = window && (self.running_export.is_some() || !self.export_queue.is_empty());
            if busy {
                ui.label("还有后台导出任务，保存后会等待任务结束再退出。");
            }
            if !self.error.is_empty() {
                ui.colored_label(BAD, &self.error);
            }
            ui.horizontal(|ui| {
                if ui
                    .button(if busy {
                        "保存全部，完成导出后退出"
                    } else {
                        "保存并继续"
                    })
                    .clicked()
                {
                    save = true;
                }
                if !busy && ui.button("放弃未保存改动").clicked() {
                    discard = true;
                }
                if ui.button("取消").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.pending_close = None;
            return;
        }
        if save {
            let ok = match self.pending_close {
                Some(PendingClose::Tab(i)) => self.tabs[i].save(),
                _ => self.save_all(),
            };
            if !ok {
                return;
            }
        }
        if save || discard {
            match self.pending_close.take() {
                Some(PendingClose::Tab(i)) => self.close_tab(i),
                Some(PendingClose::Window) => {
                    if discard {
                        for t in &self.tabs {
                            t.discard_recovery();
                        }
                    }
                    self.save_session();
                    let _ = self.catalog.save();
                    if self.running_export.is_some() || !self.export_queue.is_empty() {
                        self.quit_after_exports = true;
                    } else {
                        self.allow_exit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                None => {}
            }
        }
    }
}
impl App for Workspace {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll(ctx);
        if self.tasks_open && self.pending_close.is_none() {
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::S)
            }) {
                self.save_all();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
                && let Some(tab) = self.tabs.get_mut(self.task_project)
            {
                tab.save();
            }
        }
        if !self.allow_exit && ctx.input(|i| i.viewport().close_requested()) {
            if self.tabs.iter().any(Studio::dirty)
                || self.running_export.is_some()
                || !self.export_queue.is_empty()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_close = Some(PendingClose::Window);
            } else {
                if self.session_changed {
                    self.save_session();
                }
                let _ = self.catalog.save();
            }
        }
        if self.pending_close.is_none() && !self.new_open && !self.tasks_open && !self.jobs_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
                self.new_open = true;
                self.fork_source = None;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
                self.open_dialog(ctx);
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::S)
            }) {
                self.save_all();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::W))
                && let Some(i) = self.active
            {
                if self.tabs[i].dirty() {
                    self.pending_close = Some(PendingClose::Tab(i));
                } else {
                    self.close_tab(i);
                }
            }
            if !self.tabs.is_empty()
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Tab))
            {
                self.activate((self.active.unwrap_or(0) + 1) % self.tabs.len());
            }
        }
        self.tabs_bar(ctx);
        if self.home || self.active.is_none() {
            self.center(ctx);
        } else if let Some(i) = self.active {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "{} — OpenGalgame",
                self.tabs[i].project().title
            )));
            self.tabs[i].external_modal =
                self.pending_close.is_some() || self.new_open || self.tasks_open || self.jobs_open;
            self.tabs[i].update(ctx, frame);
            if let Some(request) = self.tabs[i].workspace_request.take() {
                match request {
                    WorkspaceRequest::New => {
                        self.new_open = true;
                        self.fork_source = None;
                    }
                    WorkspaceRequest::Open(path) => self.open(path, true, ctx),
                    WorkspaceRequest::Export {
                        project,
                        base,
                        destination,
                    } => self.queue_export(*project, base, destination, ctx),
                }
            }
        }
        if self.home {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                "OpenGalgame · 项目中心".into(),
            ));
        }
        self.new_project_window(ctx);
        self.task_window(ctx);
        self.job_window(ctx);
        self.close_window(ctx);
    }
}
