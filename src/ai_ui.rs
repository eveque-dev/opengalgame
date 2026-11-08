use crate::{
    ai::{self, Connection, Preferences, Provider, Reply},
    editor_theme,
    studio::Studio,
};
use eframe::egui::{self, RichText, TextEdit};
use std::{path::PathBuf, sync::mpsc};

mod story;
use story::{ProjectScope, StoryState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Writing,
    Context,
    Branches,
    Audit,
    Settings,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Polish,
    Continue,
    Ideas,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Polish => "润色对白",
            Self::Continue => "续写对白",
            Self::Ideas => "灵感草稿",
        }
    }

    fn instruction(self) -> &'static str {
        match self {
            Self::Polish => {
                "润色所附视觉小说对白。保持语言、人称、人物语气和剧情事实。只返回完整替换文本。"
            }
            Self::Continue => {
                "续写所附视觉小说对白。遵循原文语言和人物语气。只返回新增正文，不复述原文。"
            }
            Self::Ideas => "为视觉小说作者构思草稿，按作者要求作答。若无语言要求，使用中文。",
        }
    }
}

struct Scene {
    path: PathBuf,
    project: String,
    id: String,
    title: String,
    original: String,
}

impl Scene {
    fn capture(studio: &Studio) -> Option<Self> {
        let node = studio.selected_scene()?;
        Some(Self {
            path: studio.path().to_owned(),
            project: studio.project().id.clone(),
            id: node.id.clone(),
            title: node.title.clone(),
            original: node.text.clone(),
        })
    }

    fn matches(&self, studio: &Studio) -> bool {
        studio.path() == self.path
            && studio.project().id == self.project
            && studio.selected_scene().is_some_and(|node| {
                node.id == self.id && node.text == self.original && node.title == self.title
            })
    }
}

struct Draft {
    mode: Mode,
    source: Option<Scene>,
    reply: Reply,
    applied: bool,
}

enum Job {
    Test,
    Draft { mode: Mode, source: Option<Scene> },
    Branch(ProjectScope),
}

pub struct Assistant {
    pub open: bool,
    page: Page,
    story: StoryState,
    connection: Connection,
    key: String,
    remember: bool,
    mode: Mode,
    prompt: String,
    attach_scene: bool,
    pending: Option<(Job, mpsc::Receiver<anyhow::Result<Reply>>)>,
    draft: Option<Draft>,
    message: Option<(bool, String)>,
}

impl Assistant {
    pub fn load() -> Self {
        let mut message = None;
        let prefs = Preferences::load().unwrap_or_else(|_| {
            message = Some((false, "无法读取上次的 AI 设置，请重新填写。".into()));
            Preferences::default()
        });
        let key = prefs.key().unwrap_or_else(|_| {
            message = Some((false, "无法读取已保存的密钥，请重新填写。".into()));
            None
        });
        let remember = key.is_some();
        let key = key.unwrap_or_default();
        Self {
            open: false,
            page: if prefs.connection.validate(&key).is_err() {
                Page::Settings
            } else {
                Page::Writing
            },
            story: StoryState::default(),
            connection: prefs.connection,
            key,
            remember,
            mode: Mode::default(),
            prompt: String::new(),
            attach_scene: false,
            pending: None,
            draft: None,
            message,
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.story.running.is_some()
    }

    fn start(&mut self, ctx: &egui::Context, job: Job, instruction: &'static str, input: String) {
        let connection = self.connection.clone();
        let key = self.key.clone();
        let testing = matches!(&job, Job::Test);
        let ctx = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("story-ai".into())
            .spawn(move || {
                let result = ai::complete(&connection, &key, instruction, &input, testing);
                let _ = sender.send(result);
                ctx.request_repaint();
            }) {
            Ok(_) => {
                self.pending = Some((job, receiver));
                self.message = None;
            }
            Err(_) => self.message = Some((false, "无法启动网络请求，请重试。".into())),
        }
    }

    fn poll(&mut self) {
        let Some((_, receiver)) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(anyhow::anyhow!("请求中断，请重试。")),
        };
        let (job, _) = self.pending.take().unwrap();
        match (job, result) {
            (Job::Test, Ok(_)) => self.message = Some((true, "已收到模型回复，连接成功。".into())),
            (Job::Draft { mode, source }, Ok(reply)) => {
                self.draft = Some(Draft {
                    mode,
                    source,
                    reply,
                    applied: false,
                });
            }
            (Job::Branch(scope), Ok(reply)) => self.story.receive_branch(scope, reply),
            (_, Err(error)) => self.message = Some((false, error.to_string())),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, studio: Option<&mut Studio>) {
        self.poll();
        self.story.poll_audit();
        self.story.track_project(studio.as_deref());
        if !self.open {
            return;
        }
        let mut open = self.open;
        let screen = ctx.content_rect();
        egui::Window::new("AI 创作助手")
            .open(&mut open)
            .default_width(680.)
            .default_height(600.)
            .default_pos(screen.center_top() + egui::vec2(-345., 100.))
            .max_width(850.)
            .max_height(screen.height() * 0.8)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (page, name) in [
                        (Page::Writing, "创作"),
                        (Page::Context, "人物与前情"),
                        (Page::Branches, "大纲分支"),
                        (Page::Audit, "全剧检查"),
                        (Page::Settings, "连接设置"),
                    ] {
                        if ui.selectable_value(&mut self.page, page, name).changed() {
                            self.message = None;
                        }
                    }
                    if self.busy() {
                        ui.spinner();
                        ui.label("等待模型回复…");
                    }
                });
                ui.separator();
                match self.page {
                    Page::Writing => self.writing_ui(ui, studio),
                    Page::Context => self.story.edit_context(ui, studio),
                    Page::Branches => self.branch_ui(ui, studio),
                    Page::Audit => self.audit_ui(ui, studio),
                    Page::Settings => self.connection_ui(ui),
                }
                if let Some((ok, message)) = &self.message {
                    ui.colored_label(editor_theme::feedback(*ok), message);
                }
            });
        self.open = open;
    }

    fn connection_ui(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy();
        ui.add_enabled_ui(!busy, |ui| {
            let previous = self.connection.provider;
            ui.horizontal(|ui| {
                ui.label("服务");
                egui::ComboBox::from_id_salt("ai-provider")
                    .selected_text(previous.label())
                    .show_ui(ui, |ui| {
                        for provider in Provider::ALL {
                            ui.selectable_value(
                                &mut self.connection.provider,
                                provider,
                                provider.label(),
                            );
                        }
                    });
            });
            if self.connection.provider != previous {
                self.connection = Connection::for_provider(self.connection.provider);
                self.key.clear();
                self.remember = false;
                self.message = None;
            }
            let provider = self.connection.provider;
            let label = ui.label("API 地址");
            if let Some(base) = provider.fixed_base() {
                ui.monospace(base);
            } else {
                let hint = if provider == Provider::Qwen {
                    "填写百炼控制台显示的工作空间兼容地址"
                } else {
                    "例如 http://localhost:11434/v1"
                };
                if ui
                    .add(
                        TextEdit::singleline(&mut self.connection.base)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY)
                            .char_limit(2048),
                    )
                    .labelled_by(label.id)
                    .changed()
                {
                    self.key.clear();
                    self.remember = false;
                    self.message = None;
                }
            }
            let model = ui.label("模型标识");
            ui.add(
                TextEdit::singleline(&mut self.connection.model)
                    .desired_width(f32::INFINITY)
                    .char_limit(240),
            )
            .labelled_by(model.id);
            let key = ui.label("API 密钥");
            ui.add(
                TextEdit::singleline(&mut self.key)
                    .password(true)
                    .desired_width(f32::INFINITY)
                    .char_limit(4096),
            )
            .labelled_by(key.id);
            ui.checkbox(&mut self.remember, "在当前 Windows 账户中加密保存密钥");
            let validation = self.connection.validate(&self.key);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(validation.is_ok(), egui::Button::new("测试连接"))
                    .clicked()
                {
                    self.start(ui.ctx(), Job::Test, "只回复 OK。", "连接测试".into());
                }
                if ui
                    .add_enabled(
                        validation.is_ok()
                            || (!self.remember && self.connection.validate_config().is_ok()),
                        egui::Button::new("保存设置"),
                    )
                    .clicked()
                {
                    self.message = Some(
                        match Preferences::save(&self.connection, &self.key, self.remember) {
                            Ok(()) => (true, "连接设置已保存到本机。".into()),
                            Err(error) => (false, format!("保存失败。{error}")),
                        },
                    );
                }
            });
            if let Err(error) = validation {
                ui.label(RichText::new(error.to_string()).weak());
            }
            ui.label(
                RichText::new("测试连接只发送一条简短测试消息，服务可能按用量计费。")
                    .small()
                    .weak(),
            );
            ui.collapsing("高级选项", |ui| {
                ui.horizontal(|ui| {
                    ui.label("输出上限（tokens）");
                    ui.add(
                        egui::DragValue::new(&mut self.connection.output_limit).range(256..=8192),
                    );
                });
                if provider == Provider::Custom {
                    ui.checkbox(
                        &mut self.connection.modern_token_limit,
                        "使用 max_completion_tokens",
                    );
                }
            });
        });
    }

    fn writing_ui(&mut self, ui: &mut egui::Ui, studio: Option<&mut Studio>) {
        ui.label(format!(
            "{} · {}",
            self.connection.provider.label(),
            self.connection.model
        ));
        let scene = studio.as_deref().and_then(Studio::selected_scene);
        let busy = self.busy();
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                for mode in [Mode::Polish, Mode::Continue, Mode::Ideas] {
                    ui.selectable_value(&mut self.mode, mode, mode.label());
                }
            });
            if self.mode == Mode::Ideas {
                ui.add_enabled(scene.is_some(), egui::Checkbox::new(&mut self.attach_scene, "附上当前节点标题与对白"));
            }
            let use_scene = self.mode != Mode::Ideas || self.attach_scene && scene.is_some();
            let prompt = ui.label("创作要求");
            ui.add(TextEdit::multiline(&mut self.prompt).desired_rows(4)
                .desired_width(f32::INFINITY).char_limit(6000)
                .hint_text("例如：保持克制的语气，突出两人之间的犹豫。")).labelled_by(prompt.id);
            if use_scene {
                if let Some(node) = scene {
                    ui.collapsing(format!("将发送当前场景 · {}", node.title), |ui| {
                        ui.label(&node.text);
                    });
                } else {
                    ui.label("请打开项目并选中要编辑的节点。");
                }
            }
            if let Some(studio) = studio.as_deref() {
                self.story.pick_context(ui, studio.project());
            }
            let packet = studio.as_deref()
                .map(|studio| self.story.context(studio.project())).unwrap_or_default();
            let input = serde_json::json!({
                "要求": self.prompt.trim(),
                "场景": scene.filter(|_| use_scene).map(|node| serde_json::json!({"标题": node.title, "对白": node.text})),
                "context": packet,
            }).to_string();
            story::preview_request(ui, &input);
            let endpoint = self.connection.validate(&self.key);
            if let Ok(url) = &endpoint {
                ui.label(RichText::new(format!("本次内容将发往 {}", url.host_str().unwrap_or_default())).small().weak());
            } else {
                ui.label("请在连接设置中填写模型与密钥。");
            }
            let ready = endpoint.is_ok() && input.len() <= opengalgame::story_tools::INPUT_LIMIT
                && (!use_scene || scene.is_some())
                && (self.mode != Mode::Ideas || !self.prompt.trim().is_empty())
                && (self.mode != Mode::Polish || scene.is_some_and(|node| !node.text.trim().is_empty()));
            if ui.add_enabled(ready, editor_theme::primary_button(ui, "生成草稿")).clicked() {
                let source = if use_scene { studio.as_deref().and_then(Scene::capture) } else { None };
                self.start(ui.ctx(), Job::Draft { mode: self.mode, source }, self.mode.instruction(), input);
            }
        });

        let Some(draft) = &mut self.draft else {
            return;
        };
        ui.separator();
        ui.heading("草稿");
        if let Some(source) = &draft.source {
            ui.label(format!("{} · {}", draft.mode.label(), source.title));
        }
        if draft.reply.truncated {
            ui.colored_label(
                editor_theme::feedback(false),
                "模型达到输出上限，草稿可能未写完。",
            );
        }
        if let Some(tokens) = draft.reply.tokens {
            ui.label(
                RichText::new(format!("本次用量 {tokens} tokens"))
                    .small()
                    .weak(),
            );
        }
        ui.add(
            TextEdit::multiline(&mut draft.reply.text)
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        let has_text = !draft.reply.text.trim().is_empty();
        let can_apply = draft.mode != Mode::Ideas
            && !draft.applied
            && !busy
            && draft
                .source
                .as_ref()
                .zip(studio.as_deref())
                .is_some_and(|(source, studio)| source.matches(studio));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(has_text, egui::Button::new("复制草稿"))
                .clicked()
            {
                ui.ctx().copy_text(draft.reply.text.clone());
                self.message = Some((true, "草稿已复制。".into()));
            }
            if draft.mode != Mode::Ideas
                && ui
                    .add_enabled(
                        can_apply && has_text,
                        egui::Button::new(if draft.mode == Mode::Polish {
                            "替换当前对白"
                        } else {
                            "追加到当前对白"
                        }),
                    )
                    .clicked()
                && let (Some(studio), Some(source)) = (studio, draft.source.as_ref())
            {
                let text = draft.reply.text.trim();
                let replacement =
                    if draft.mode == Mode::Continue && !source.original.trim().is_empty() {
                        let separator = if source.original.ends_with('\n') {
                            ""
                        } else {
                            "\n"
                        };
                        format!("{}{separator}{text}", source.original)
                    } else {
                        text.to_owned()
                    };
                studio.edit_external(|project| {
                    if let Some(node) = project.nodes.iter_mut().find(|node| node.id == source.id) {
                        node.text = replacement;
                    }
                });
                draft.applied = true;
                self.message = Some((true, "已写入对白，可以撤销。".into()));
            }
        });
        if draft.mode != Mode::Ideas && !draft.applied && !can_apply && !busy {
            ui.label("当前节点或原对白已变化。可以复制草稿，或用当前对白重新生成。");
        }
    }
}
