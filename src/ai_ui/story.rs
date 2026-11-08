use super::*;
use opengalgame::{
    model::{NodeKind, Project},
    story_tools::{
        AuditBatch, AuditFinding, AuditPlan, BranchDraft, ContextPacket, INPUT_LIMIT, decode_json,
        selected_context,
    },
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

const BRANCH_PROMPT: &str = r#"把用户的大纲变为视觉小说分支。只输出 JSON 对象，不加解释。
格式为 {"entry":"a","scenes":[{"id":"a","title":"标题","kind":"choice","text":"正文",
"character":null,"next":null,"choices":[{"text":"选择","target":"b"}]}]}。
kind 只能是 dialogue、choice、ending。dialogue 只能有 next，choice 需要 2～6 个 choices，ending 没有跳转。
所有跳转都指向草稿内节点，所有节点从 entry 可达且可抵达结局。
至少有一个分支选择，默认写 4～8 个简短节点，最多 24 个。id 只用 ASCII 字母数字、下划线或连字符。
character 只用提供的人物标识，旁白或新人物使用 null。忠实于大纲和已提供的角色设定。"#;

const AUDIT_PROMPT: &str = r#"检查视觉小说剧本中的人设、口吻、时间线、事实和分支逻辑。
依据实际节点和作者勾选的设定，区分互斥路线与同一路线的矛盾。
上一批摘要用于跨批核对，不把摘要缺失当作矛盾。只输出 JSON 对象。
格式为 {"findings":[{"category":"人设","nodes":["已提供的节点标识"],
"title":"疑点","evidence":"引用正文或说明跳转证据","suggestion":"供作者核对的建议"}],
"memo":"保留迄今关键人物事实、时间顺序、分支与证据节点，供下一批检查"}。
只报有证据的疑点，最多 6 条，无法确认就使用空数组。memo 限 900 个汉字，保留关键节点标识。"#;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ProjectScope {
    path: PathBuf,
    id: String,
    revision: u64,
}

impl ProjectScope {
    fn capture(studio: &Studio) -> Self {
        Self {
            path: studio.path().to_owned(),
            id: studio.project().id.clone(),
            revision: studio.revision,
        }
    }

    fn belongs_to(&self, studio: &Studio) -> bool {
        self.path == studio.path() && self.id == studio.project().id
    }

    fn current(&self, studio: &Studio) -> bool {
        self.belongs_to(studio) && self.revision == studio.revision
    }
}

struct BranchResult {
    scope: ProjectScope,
    raw: String,
    draft: Option<BranchDraft>,
    decode_error: Option<String>,
    truncated: bool,
    tokens: Option<u64>,
    inserted: bool,
}

impl BranchResult {
    fn parse(&mut self) {
        match decode_json::<BranchDraft>(&self.raw).and_then(|draft| {
            anyhow::ensure!(
                draft.scenes.len() <= opengalgame::story_tools::BRANCH_SCENE_LIMIT
                    && draft.scenes.iter().all(|scene| scene.choices.len() <= 6),
                "预览最多支持 24 个节点，每个节点最多 6 个选项。"
            );
            Ok(draft)
        }) {
            Ok(draft) => {
                self.draft = Some(draft);
                self.decode_error = None;
            }
            Err(error) => self.decode_error = Some(error.to_string()),
        }
    }
}

struct PreparedAudit {
    scope: ProjectScope,
    plan: Arc<AuditPlan>,
}

pub(super) struct RunningAudit {
    result: mpsc::Receiver<AuditReport>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    total: usize,
}

struct AuditReport {
    scope: ProjectScope,
    total: usize,
    completed: usize,
    findings: Vec<AuditFinding>,
    skipped: usize,
    tokens: u64,
    stopped: bool,
    error: Option<String>,
}

#[derive(Default)]
pub(super) struct StoryState {
    project_key: Option<(PathBuf, String)>,
    selected_character: Option<String>,
    include_recap: bool,
    cards: BTreeSet<String>,
    outline: String,
    branch: Option<BranchResult>,
    connect: bool,
    option_label: String,
    prepared: Option<PreparedAudit>,
    preview_batch: usize,
    pub(super) running: Option<RunningAudit>,
    report: Option<AuditReport>,
}

impl StoryState {
    pub(super) fn track_project(&mut self, studio: Option<&Studio>) {
        let unchanged = match (&self.project_key, studio) {
            (Some((path, id)), Some(studio)) => path == studio.path() && id == &studio.project().id,
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.project_key = studio.map(|s| (s.path().to_owned(), s.project().id.clone()));
            self.selected_character = None;
            self.include_recap = false;
            self.cards.clear();
            self.prepared = None;
            self.connect = false;
        }
    }

    pub(super) fn edit_context(&mut self, ui: &mut egui::Ui, studio: Option<&mut Studio>) {
        let Some(studio) = studio.filter(|studio| !studio.is_playing()) else {
            ui.label("请打开要编辑的作品并退出试玩。");
            return;
        };
        ui.heading("人物设定卡");
        ui.label("资料随工程保存。发送给模型前，可按角色勾选。");
        let project = studio.project();
        if !project
            .characters
            .iter()
            .any(|c| Some(&c.id) == self.selected_character.as_ref())
        {
            self.selected_character = project.characters.first().map(|c| c.id.clone());
        }
        let mut card_change = None;
        if project.characters.is_empty() {
            ui.label("先到工作台的「人物」页创建角色。");
        } else {
            choose_id(
                ui,
                "profile-character",
                &mut self.selected_character,
                project
                    .characters
                    .iter()
                    .map(|c| (c.id.as_str(), c.name.as_str())),
                None,
            );
            if let Some(character) = project
                .characters
                .iter()
                .find(|c| Some(&c.id) == self.selected_character.as_ref())
            {
                let mut card = character.profile.clone();
                let changed = text_field(ui, "人物定位与动机", &mut card.role, 3, 1000)
                    | text_field(ui, "说话方式与常用措辞", &mut card.voice, 3, 1000)
                    | text_field(ui, "关系、已知事实与限制", &mut card.continuity, 3, 1800);
                if changed {
                    card_change = Some((character.id.clone(), card));
                }
            }
        }
        ui.separator();
        ui.heading("前情摘要");
        ui.label("记录已经发生的事、路线区别和待回收伏笔。每次请求可选择是否附带。");
        let mut recap = project.story_recap.clone();
        let recap_changed = text_field(ui, "作品前情摘要", &mut recap, 5, 6000);
        if recap_changed || card_change.is_some() {
            studio.edit_external(|project| {
                if recap_changed {
                    project.story_recap = recap;
                }
                if let Some((id, card)) = card_change
                    && let Some(character) = project.characters.iter_mut().find(|c| c.id == id)
                {
                    character.profile = card;
                }
            });
            self.prepared = None;
        }
    }

    pub(super) fn pick_context(&mut self, ui: &mut egui::Ui, project: &Project) {
        let count = project
            .characters
            .iter()
            .filter(|c| self.cards.contains(&c.id))
            .count();
        let mut changed = false;
        let recap = if self.include_recap && !project.story_recap.trim().is_empty() {
            " · 前情摘要"
        } else {
            ""
        };
        egui::CollapsingHeader::new(format!("附带创作资料 · {count} 张人物卡{recap}"))
            .id_salt("story-context-selection")
            .show(ui, |ui| {
                changed |= ui
                    .add_enabled(
                        !project.story_recap.trim().is_empty(),
                        egui::Checkbox::new(&mut self.include_recap, "附上前情摘要"),
                    )
                    .changed();
                egui::ScrollArea::vertical()
                    .id_salt("character-context-list")
                    .max_height(145.)
                    .show(ui, |ui| {
                        for character in &project.characters {
                            if character.profile == Default::default() {
                                continue;
                            }
                            let mut checked = self.cards.contains(&character.id);
                            if ui
                                .checkbox(
                                    &mut checked,
                                    format!("{} · {}", character.name, character.id),
                                )
                                .changed()
                            {
                                changed = true;
                                if checked {
                                    self.cards.insert(character.id.clone());
                                } else {
                                    self.cards.remove(&character.id);
                                }
                            }
                        }
                    });
                ui.label(RichText::new("在「人物与前情」中编辑资料。").small().weak());
            });
        if changed {
            self.prepared = None;
        }
    }

    pub(super) fn context<'a>(&self, project: &'a Project) -> ContextPacket<'a> {
        selected_context(project, self.include_recap, &self.cards)
    }

    pub(super) fn receive_branch(&mut self, scope: ProjectScope, reply: Reply) {
        let mut raw = reply.text;
        let too_long = raw.len() > 160_000;
        if too_long {
            raw.truncate(raw.floor_char_boundary(160_000));
        }
        let mut result = BranchResult {
            scope,
            raw,
            draft: None,
            decode_error: None,
            truncated: reply.truncated || too_long,
            tokens: reply.tokens,
            inserted: false,
        };
        result.parse();
        self.branch = Some(result);
        self.connect = false;
        self.option_label.clear();
    }

    pub(super) fn poll_audit(&mut self) {
        let Some(running) = &self.running else {
            return;
        };
        match running.result.try_recv() {
            Ok(report) => {
                self.report = Some(report);
                self.running = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.running = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

fn text_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    rows: usize,
    limit: usize,
) -> bool {
    let label = ui.label(label);
    ui.add(
        TextEdit::multiline(value)
            .desired_width(f32::INFINITY)
            .desired_rows(rows)
            .char_limit(limit),
    )
    .labelled_by(label.id)
    .changed()
}

fn choose_id<'a>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    choices: impl IntoIterator<Item = (&'a str, &'a str)>,
    empty: Option<&str>,
) {
    let choices: Vec<_> = choices.into_iter().collect();
    let display = choices
        .iter()
        .find(|(id, _)| Some(*id) == value.as_deref())
        .map(|(_, title)| *title)
        .unwrap_or_else(|| value.as_deref().unwrap_or(empty.unwrap_or("请选择")));
    egui::ComboBox::from_id_salt(id)
        .selected_text(display.to_owned())
        .show_ui(ui, |ui| {
            if let Some(empty) = empty {
                ui.selectable_value(value, None, empty);
            }
            for (id, title) in choices {
                ui.selectable_value(value, Some(id.to_owned()), format!("{title} · {id}"));
            }
        });
}

pub(super) fn preview_request(ui: &mut egui::Ui, input: &str) {
    ui.collapsing("预览将发送的内容", |ui| {
        egui::ScrollArea::vertical()
            .id_salt("request-preview")
            .max_height(220.)
            .show(ui, |ui| {
                let mut preview = input;
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .desired_width(f32::INFINITY)
                        .desired_rows(4),
                );
            });
    });
    if input.len() > INPUT_LIMIT {
        ui.colored_label(
            editor_theme::feedback(false),
            "内容超过单次请求上限，请减少附加资料或缩短输入。",
        );
    }
}

impl Assistant {
    pub(super) fn branch_ui(&mut self, ui: &mut egui::Ui, studio: Option<&mut Studio>) {
        let Some(studio) = studio.filter(|studio| !studio.is_playing()) else {
            ui.label("请打开作品并退出试玩，再生成分支。");
            return;
        };
        let busy = self.busy();
        ui.heading("大纲生成分支");
        ui.label("填入剧情起点、关键选择与预期结局。草稿通过检查后才写入作品。");
        ui.add_enabled_ui(!busy, |ui| {
            text_field(ui, "故事大纲", &mut self.story.outline, 5, 8000);
            self.story.pick_context(ui, studio.project());
            let input = json!({
                "outline": self.story.outline.trim(),
                "cast": studio.project().characters.iter().map(|c| json!({"id": c.id, "name": c.name})).collect::<Vec<_>>(),
                "context": self.story.context(studio.project()),
            }).to_string();
            preview_request(ui, &input);
            let endpoint = self.connection.validate(&self.key);
            show_destination(ui, &endpoint);
            let ready = endpoint.is_ok() && !self.story.outline.trim().is_empty() && input.len() <= INPUT_LIMIT;
            if ui.add_enabled(ready, editor_theme::primary_button(ui, "生成分支草稿")).clicked() {
                self.start(ui.ctx(), Job::Branch(ProjectScope::capture(studio)), BRANCH_PROMPT, input);
            }
        });
        let Some(branch) = &mut self.story.branch else {
            return;
        };
        ui.separator();
        ui.heading("分支预览");
        if branch.truncated {
            ui.colored_label(
                editor_theme::feedback(false),
                "模型达到输出上限，可在连接设置提高上限后重新生成。",
            );
        }
        if let Some(tokens) = branch.tokens {
            ui.label(format!("本次用量 {tokens} tokens"));
        }
        ui.collapsing("查看或修订模型 JSON", |ui| {
            text_field(ui, "模型 JSON", &mut branch.raw, 6, 80_000);
            if ui
                .add_enabled(
                    !branch.inserted && !busy,
                    egui::Button::new("重新解析 JSON"),
                )
                .clicked()
            {
                branch.parse();
            }
        });
        if let Some(error) = &branch.decode_error {
            ui.colored_label(editor_theme::feedback(false), error);
        }
        let Some(draft) = &mut branch.draft else {
            return;
        };
        if branch.inserted {
            ui.label("该草稿已经导入。请在剧情工作台继续编辑。");
            return;
        }
        edit_branch(ui, draft, studio.project());
        let validation = draft.validate(studio.project());
        if let Err(error) = &validation {
            ui.colored_label(editor_theme::feedback(false), error.to_string());
        }
        let same_project = branch.scope.belongs_to(studio);
        if !same_project {
            ui.label("请回到生成草稿的作品后导入。");
        }
        let anchor = studio
            .selected_scene()
            .filter(|n| n.kind == NodeKind::Choice);
        ui.add_enabled(
            anchor.is_some(),
            egui::Checkbox::new(&mut self.story.connect, "在当前选择节点新增一个选项"),
        );
        let attach = if self.story.connect {
            text_field(ui, "新选项文字", &mut self.story.option_label, 1, 120);
            if anchor.is_none() {
                ui.label("请在剧情目录选择要接入的选择节点。");
            }
            anchor.map(|node| (node.id.clone(), self.story.option_label.trim().to_owned()))
        } else {
            ui.label("导入为独立分支。可在剧情分支图中把入口接到现有路线。");
            None
        };
        let ready = same_project
            && !busy
            && validation.is_ok()
            && branch.decode_error.is_none()
            && (!self.story.connect || attach.as_ref().is_some_and(|(_, label)| !label.is_empty()));
        if ui
            .add_enabled(ready, editor_theme::primary_button(ui, "导入可编辑分支"))
            .clicked()
        {
            let result = studio.edit_external(|project| {
                draft.insert(
                    project,
                    attach
                        .as_ref()
                        .map(|(id, label)| (id.as_str(), label.as_str())),
                )
            });
            match result {
                Ok(entry) => {
                    studio.focus_scene(&entry);
                    branch.inserted = true;
                    self.message = Some((
                        true,
                        format!("已导入 {} 个节点，可以撤销。", draft.scenes.len()),
                    ));
                }
                Err(error) => self.message = Some((false, error.to_string())),
            }
        }
    }
}

fn edit_branch(ui: &mut egui::Ui, draft: &mut BranchDraft, project: &Project) {
    ui.label(format!(
        "{} 个节点 · 可修改标题、正文、人物和跳转",
        draft.scenes.len()
    ));
    let options: Vec<_> = draft
        .scenes
        .iter()
        .map(|scene| (scene.id.clone(), scene.title.clone()))
        .collect();
    let destinations = || {
        options
            .iter()
            .map(|(id, title)| (id.as_str(), title.as_str()))
    };
    let mut entry = Some(draft.entry.clone());
    ui.horizontal(|ui| {
        ui.label("草稿入口");
        choose_id(ui, "branch-entry", &mut entry, destinations(), None);
    });
    if let Some(entry) = entry {
        draft.entry = entry;
    }
    for (index, scene) in draft.scenes.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            egui::CollapsingHeader::new(format!(
                "{} · {} · {}",
                scene.kind.label(),
                scene.title,
                scene.id
            ))
            .id_salt("branch-scene")
            .show(ui, |ui| {
                text_field(ui, "节点标题", &mut scene.title, 1, 100);
                text_field(ui, "对白正文", &mut scene.text, 3, 8000);
                ui.horizontal(|ui| {
                    ui.label("发言人");
                    choose_id(
                        ui,
                        "speaker",
                        &mut scene.character,
                        project
                            .characters
                            .iter()
                            .map(|c| (c.id.as_str(), c.name.as_str())),
                        Some("旁白"),
                    );
                });
                if scene.kind == NodeKind::Dialogue {
                    ui.label("读完后前往");
                    choose_id(ui, "next", &mut scene.next, destinations(), None);
                }
                for (index, choice) in scene.choices.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        text_field(ui, "选项文字", &mut choice.text, 1, 120);
                        let mut target = Some(choice.target.clone());
                        choose_id(ui, "target", &mut target, destinations(), None);
                        if let Some(target) = target {
                            choice.target = target;
                        }
                    });
                }
            });
        });
    }
}

fn show_destination(ui: &mut egui::Ui, endpoint: &anyhow::Result<reqwest::Url>) {
    match endpoint {
        Ok(url) => ui.label(
            RichText::new(format!("请求将发往 {}", url.host_str().unwrap_or_default()))
                .small()
                .weak(),
        ),
        Err(_) => ui.label("请先在「连接设置」中完成配置。"),
    };
}

impl Assistant {
    pub(super) fn audit_ui(&mut self, ui: &mut egui::Ui, studio: Option<&mut Studio>) {
        let Some(studio) = studio else {
            ui.label("请打开要检查的作品。");
            return;
        };
        ui.heading("全剧一致性检查");
        ui.label("核对人设、时间线、剧情事实和分支逻辑。疑点会附证据和可定位节点。");
        let busy = self.busy();
        ui.add_enabled_ui(!busy, |ui| {
            self.story.pick_context(ui, studio.project());
            if ui.button("准备全剧检查").clicked() {
                let context = self.story.context(studio.project());
                match AuditPlan::prepare(studio.project(), &context) {
                    Ok(plan) => {
                        self.story.prepared = Some(PreparedAudit {
                            scope: ProjectScope::capture(studio),
                            plan: Arc::new(plan),
                        });
                        self.story.preview_batch = 0;
                        self.message = None;
                    }
                    Err(error) => {
                        self.story.prepared = None;
                        self.message = Some((false, error.to_string()));
                    }
                }
            }
        });
        if let Some(prepared) = &self.story.prepared {
            let endpoint = self.connection.validate(&self.key);
            let current = prepared.scope.current(studio);
            ui.group(|ui| {
                ui.strong(format!(
                    "{} 个节点 · {} 次模型请求",
                    prepared.plan.node_ids.len(),
                    prepared.plan.len(),
                ));
                ui.label("发送全部剧本文字、跳转、变量和人物姓名。所勾选的设定卡和前情也会附带。");
                if prepared.plan.len() > 1 {
                    ui.label("后续批次带上前一批生成的摘要。跨批疑点需要结合原文复核。");
                }
                ui.horizontal(|ui| {
                    ui.label("预览批次");
                    ui.add(
                        egui::DragValue::new(&mut self.story.preview_batch)
                            .range(0..=prepared.plan.len() - 1)
                            .custom_formatter(|value, _| (value as usize + 1).to_string())
                            .custom_parser(|value| {
                                value
                                    .parse::<usize>()
                                    .ok()
                                    .map(|n| n.saturating_sub(1) as f64)
                            }),
                    );
                });
                let input = prepared.plan.input(self.story.preview_batch, "");
                preview_request(ui, &input);
                show_destination(ui, &endpoint);
                if !current {
                    ui.label("工程已改动。请重新准备检查。");
                }
            });
            if ui
                .add_enabled(
                    current && !busy && endpoint.is_ok(),
                    editor_theme::primary_button(ui, "开始检查"),
                )
                .clicked()
            {
                self.start_audit(ui.ctx());
            }
        }
        if let Some(running) = &self.story.running {
            let done = running.progress.load(Ordering::Relaxed);
            ui.add(
                egui::ProgressBar::new(done as f32 / running.total as f32)
                    .text(format!("已检查 {done} / {} 批", running.total)),
            );
            let cancelled = running.cancel.load(Ordering::Relaxed);
            if ui
                .add_enabled(!cancelled, egui::Button::new("停止后续批次"))
                .clicked()
            {
                running.cancel.store(true, Ordering::Relaxed);
            }
            if cancelled {
                ui.label("等待当前请求结束。已返回的结果会保留。");
            }
        }
        if let Some(report) = &self.story.report {
            render_report(ui, report, studio);
        }
    }

    fn start_audit(&mut self, ctx: &egui::Context) {
        let Some(prepared) = &self.story.prepared else {
            return;
        };
        let plan = Arc::clone(&prepared.plan);
        let scope = prepared.scope.clone();
        let total = plan.len();
        let connection = self.connection.clone();
        let key = self.key.clone();
        let (sender, result) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_cancel = Arc::clone(&cancel);
        let worker_progress = Arc::clone(&progress);
        let ctx = ctx.clone();
        let worker = std::thread::Builder::new()
            .name("story-audit".into())
            .spawn(move || {
                let report = audit_batches(scope, &plan, &worker_cancel, |index, input| {
                    worker_progress.store(index, Ordering::Relaxed);
                    ctx.request_repaint();
                    ai::complete(&connection, &key, AUDIT_PROMPT, input, false)
                });
                let _ = sender.send(report);
                ctx.request_repaint();
            });
        match worker {
            Ok(_) => {
                self.story.running = Some(RunningAudit {
                    result,
                    cancel,
                    progress,
                    total,
                });
                self.story.prepared = None;
                self.story.report = None;
                self.message = None;
            }
            Err(_) => self.message = Some((false, "无法启动检查，请重试。".into())),
        }
    }
}

fn audit_batches(
    scope: ProjectScope,
    plan: &AuditPlan,
    cancel: &AtomicBool,
    mut complete: impl FnMut(usize, &str) -> anyhow::Result<Reply>,
) -> AuditReport {
    let mut report = AuditReport {
        scope,
        total: plan.len(),
        completed: 0,
        findings: vec![],
        skipped: 0,
        tokens: 0,
        stopped: false,
        error: None,
    };
    let mut memo = String::new();
    let mut seen = BTreeSet::new();
    for index in 0..plan.len() {
        if cancel.load(Ordering::Relaxed) {
            report.stopped = true;
            break;
        }
        let input = plan.input(index, &memo);
        let result = complete(index, &input).and_then(|reply| {
            report.tokens = report.tokens.saturating_add(reply.tokens.unwrap_or(0));
            anyhow::ensure!(
                !reply.truncated,
                "模型回复达到输出上限，请提高输出上限后重试。"
            );
            decode_json::<AuditBatch>(&reply.text)
        });
        let batch = match result {
            Ok(batch) => batch,
            Err(error) => {
                report.error = Some(format!("第 {} 批未完成。{error}", index + 1));
                break;
            }
        };
        report.skipped += batch.findings.len().saturating_sub(24);
        for mut finding in batch.findings.into_iter().take(24) {
            if !finding.is_locatable(&plan.node_ids) {
                report.skipped += 1;
                continue;
            }
            finding.nodes.sort();
            finding.nodes.dedup();
            if seen.insert((finding.nodes.clone(), finding.title.trim().to_owned())) {
                report.findings.push(finding);
            }
        }
        memo = batch.memo;
        report.completed += 1;
    }
    report
}

fn render_report(ui: &mut egui::Ui, report: &AuditReport, studio: &mut Studio) {
    ui.separator();
    ui.heading("检查报告");
    if !report.scope.belongs_to(studio) {
        ui.label("请回到生成此报告的作品查看结果。");
        return;
    }
    let summary = format!(
        "已检查 {}/{} 批 · {} 条疑点",
        report.completed,
        report.total,
        report.findings.len()
    );
    ui.strong(&summary);
    if !report.scope.current(studio) {
        ui.label("工程已改动。本报告基于检查时的版本。");
    }
    if report.tokens > 0 {
        ui.label(format!("服务已返回的用量合计 {} tokens", report.tokens));
    }
    if let Some(error) = &report.error {
        ui.colored_label(editor_theme::feedback(false), error);
    }
    if report.stopped {
        ui.label("检查已停止，以下为已完成批次的结果。");
    }
    if report.skipped > 0 {
        ui.label(format!(
            "模型有 {} 条返回缺少有效证据或节点标识，未列入疑点。",
            report.skipped
        ));
    }
    if report.findings.is_empty() && report.completed == report.total {
        ui.label("模型未列出可定位的疑点。");
    }
    if ui.button("复制检查报告").clicked() {
        let mut text = format!("全剧一致性检查\n{summary}\n");
        if let Some(error) = &report.error {
            text.push_str(&format!("{error}\n"));
        }
        for finding in &report.findings {
            text.push_str(&format!(
                "\n{} · {}\n节点 {}\n证据 {}\n建议 {}\n",
                finding.category,
                finding.title,
                finding.nodes.join("、"),
                finding.evidence,
                finding.suggestion,
            ));
        }
        ui.ctx().copy_text(text);
    }
    let mut destination = None;
    for (index, finding) in report.findings.iter().enumerate() {
        egui::CollapsingHeader::new(format!("{} · {}", finding.category, finding.title))
            .id_salt(("audit-finding", index))
            .default_open(index == 0)
            .show(ui, |ui| {
                ui.strong("证据");
                ui.label(&finding.evidence);
                ui.strong("建议");
                ui.label(&finding.suggestion);
                ui.horizontal_wrapped(|ui| {
                    for id in &finding.nodes {
                        let node = studio.project().node(id);
                        let title = node.map(|node| node.title.as_str()).unwrap_or(id);
                        if ui
                            .add_enabled(
                                node.is_some() && !studio.is_playing(),
                                egui::Button::new(format!("定位 {title}")),
                            )
                            .clicked()
                        {
                            destination = Some(id);
                        }
                    }
                });
            });
    }
    if let Some(id) = destination {
        studio.focus_scene(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_preserves_partial_results_and_reuses_memo_after_first_batch() {
        let mut project = opengalgame::catalog::template_project(
            "验收",
            opengalgame::catalog::ProjectTemplate::Blank,
        );
        project.nodes[0].text = "一段长对白。".repeat(4500);
        let plan = AuditPlan::prepare(&project, &ContextPacket::default()).unwrap();
        assert!(plan.len() > 1);
        let scope = ProjectScope {
            path: PathBuf::new(),
            id: project.id,
            revision: 0,
        };
        let cancel = AtomicBool::new(false);
        let first_reply = || {
            Reply {
            text: json!({
                "memo": "第一批已记录：信在港口。",
                "findings": [
                    {"category":"时间线","nodes":["opening"],"title":"核对信件时间","evidence":"信在港口","suggestion":"检查日期"},
                    {"category":"时间线","nodes":["ghost"],"title":"无效定位","evidence":"错误","suggestion":"检查"}
                ]
            }).to_string(),
            truncated: false,
            tokens: Some(31),
        }
        };
        let report = audit_batches(scope.clone(), &plan, &cancel, |index, input| {
            if index == 0 {
                Ok(first_reply())
            } else {
                assert!(input.contains("第一批已记录：信在港口。"));
                Err(anyhow::anyhow!("模拟限流"))
            }
        });
        assert_eq!(
            (report.completed, report.findings.len(), report.skipped),
            (1, 1, 1)
        );
        assert!(report.error.unwrap().contains("第 2 批"));
        assert_eq!(report.tokens, 31);
        let cancelled = audit_batches(scope, &plan, &cancel, |index, _| {
            assert_eq!(index, 0);
            cancel.store(true, Ordering::Relaxed);
            Ok(first_reply())
        });
        assert!(cancelled.stopped);
        assert_eq!(cancelled.completed, 1);
    }
}
