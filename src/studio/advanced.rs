use super::*;
use opengalgame::{
    authoring::{self, Port},
    recovery,
    sound::SoundEngine,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

fn save_age(saved_at: u128) -> String {
    if saved_at == 0 {
        return "旧版存档".into();
    }
    let seconds = stamp().saturating_sub(saved_at) / 1000;
    match seconds {
        0..=59 => "刚刚保存".into(),
        60..=3599 => format!("{} 分钟前", seconds / 60),
        3600..=86399 => format!("{} 小时前", seconds / 3600),
        _ => format!("{} 天前", seconds / 86400),
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct PlayerSettings {
    pub font: f32,
    pub cps: f32,
    pub music: f32,
    pub effects: f32,
    pub reduced_motion: bool,
}
impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            font: 22.,
            cps: 0.,
            music: 0.35,
            effects: 0.65,
            reduced_motion: false,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Profile {
    settings: PlayerSettings,
    unlocked: BTreeSet<String>,
}
pub(super) struct Advanced {
    pub bulk_open: bool,
    pub bulk: String,
    pub chapter: String,
    pub chapter_filter: String,
    pub replace_open: bool,
    pub find: String,
    pub replacement: String,
    pub focus: bool,
    pub recent: Vec<PathBuf>,
    pub recovery: Option<recovery::Snapshot>,
    pub checkpoint_at: Instant,
    pub checkpoint_revision: u64,
    pub background_page: usize,
    pub link_drag: Option<Port>,
    pub link_active: bool,
    pub edge: Option<Port>,
    pub title_screen: bool,
    pub settings_open: bool,
    pub collection_open: bool,
    pub settings: PlayerSettings,
    pub unlocked: BTreeSet<String>,
    pub sound: SoundEngine,
    pub preview_audio: bool,
    pub scene_key: String,
    pub scene_at: Instant,
    pub revealed: bool,
    pub text_incomplete: bool,
    pub previous: Option<Node>,
    pub latest: Option<Node>,
    pub transitioning: bool,
    pub capture: Option<(String, u8)>,
    pub delete_asset: Option<(String, String)>,
}
impl Advanced {
    pub fn new(project: &Project, path: &Path, player: bool) -> Self {
        if !player {
            let _ = recovery::remember(path);
        }
        let profile: Profile = std::fs::read(save_directory(&project.id).join("profile.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Profile {
                settings: PlayerSettings {
                    font: project.presentation.font_size,
                    cps: project.presentation.typewriter_cps,
                    ..Default::default()
                },
                ..Default::default()
            });
        Self {
            bulk_open: false,
            bulk: String::new(),
            chapter: "第一章".into(),
            chapter_filter: String::new(),
            replace_open: false,
            find: String::new(),
            replacement: String::new(),
            focus: false,
            recent: recovery::recent().files,
            recovery: if player {
                None
            } else {
                recovery::candidate(path, project)
            },
            checkpoint_at: Instant::now(),
            checkpoint_revision: 0,
            background_page: 0,
            link_drag: None,
            link_active: false,
            edge: None,
            title_screen: player,
            settings_open: false,
            collection_open: false,
            settings: profile.settings,
            unlocked: profile.unlocked,
            sound: Default::default(),
            preview_audio: false,
            scene_key: String::new(),
            scene_at: Instant::now(),
            revealed: false,
            text_incomplete: false,
            previous: None,
            latest: None,
            transitioning: false,
            capture: None,
            delete_asset: None,
        }
    }
}
impl Studio {
    pub(crate) fn maintain_recovery(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.checkpoints.poll() {
            match result {
                Ok(()) => {
                    if !self.status_error && self.dirty() {
                        self.status = "恢复副本已写入，正式项目仍需 Ctrl+S 保存。".into();
                    }
                }
                Err(e) => self.message(Err::<String, _>(anyhow::anyhow!(e))),
            }
        }
        if !self.player_only && self.dirty() {
            ctx.request_repaint_after(Duration::from_secs(1));
            if self.advanced.checkpoint_at.elapsed() >= Duration::from_secs(30) {
                self.advanced.checkpoint_at = Instant::now();
                if self.advanced.checkpoint_revision != self.revision {
                    self.checkpoints
                        .schedule(&self.path, &self.saved, &self.project, ctx);
                    self.advanced.checkpoint_revision = self.revision;
                }
            }
        }
    }

    pub(super) fn prepare_scene(
        &mut self,
        node: &Node,
        playing: bool,
        ctx: &egui::Context,
    ) -> (Node, f32, f32) {
        let depth = self.runtime.as_ref().map(|r| r.history.len()).unwrap_or(0);
        let key = format!("{playing}|{}|{depth}", node.id);
        if self.advanced.scene_key != key {
            self.advanced.previous = self.advanced.latest.take();
            self.advanced.latest = Some(node.clone());
            self.advanced.scene_key = key;
            self.advanced.scene_at = Instant::now();
            self.advanced.revealed = false;
            self.advanced.text_incomplete = false;
            if playing && !self.advanced.settings.reduced_motion {
                ctx.request_repaint();
            }
            if playing
                && self.advanced.sound.error.is_none()
                && let Some(id) = &node.sound
                && let Some(file) = self
                    .project
                    .audio
                    .iter()
                    .find(|a| &a.id == id)
                    .map(|a| a.file.clone())
            {
                let result = safe_asset(self.base(), &file).and_then(|path| {
                    self.advanced
                        .sound
                        .effect(&path, self.advanced.settings.effects)
                });
                if let Err(e) = result {
                    self.advanced.sound.error = Some(e.to_string());
                }
            }
        }
        if playing && self.advanced.sound.error.is_none() {
            let id = node.bgm.as_ref().or(self.project.presentation.bgm.as_ref());
            let file = id
                .filter(|id| id.as_str() != "$stop")
                .and_then(|id| self.project.audio.iter().find(|a| &a.id == id))
                .map(|a| a.file.clone());
            let path = file.as_ref().and_then(|f| safe_asset(self.base(), f).ok());
            if let Err(e) = self
                .advanced
                .sound
                .music(path.as_deref(), self.advanced.settings.music)
            {
                self.advanced.sound.error = Some(e.to_string());
                self.status = "音频设备不可用，故事仍可继续，可在设置中重试。".into();
            }
        }
        let duration = self.project.presentation.transition_seconds;
        let t = if !playing || self.advanced.settings.reduced_motion || duration <= 0. {
            1.
        } else {
            (self.advanced.scene_at.elapsed().as_secs_f32() / duration).clamp(0., 1.)
        };
        self.advanced.transitioning = t < 1. && node.transition != Transition::None;
        if self.advanced.transitioning {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if node.transition == Transition::Fade && playing && !self.advanced.settings.reduced_motion
        {
            if t < 0.5
                && let Some(previous) = &self.advanced.previous
            {
                return (previous.clone(), t * 2., 0.);
            }
            return (
                node.clone(),
                if t < 0.5 { 1. - t * 2. } else { (1. - t) * 2. },
                0.,
            );
        }
        (
            node.clone(),
            0.,
            if node.transition == Transition::Slide {
                (1. - t) * 80.
            } else {
                0.
            },
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_actor(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        dialogue_h: f32,
        actor: &Actor,
        slide: f32,
        speaking: bool,
    ) {
        let file = self
            .project
            .characters
            .iter()
            .find(|c| c.id == actor.character)
            .and_then(|c| {
                actor
                    .expression
                    .as_ref()
                    .and_then(|e| c.expressions.get(e))
                    .or(c.portrait.as_ref())
            })
            .cloned();
        if let Some(file) = file
            && let Some(tex) = self.texture(ui.ctx(), &file)
        {
            let h = (rect.height() * 0.67 * actor.scale).min(rect.height() - 30.);
            let w = h * tex.aspect_ratio();
            let x = match actor.side {
                PortraitSide::Left => rect.left() + 24. - slide,
                PortraitSide::Center => rect.center().x - w / 2. + slide,
                PortraitSide::Right => rect.right() - w - 24. + slide,
            };
            let area = Rect::from_min_size(
                Pos2::new(
                    x,
                    (rect.bottom() - dialogue_h - h + 54.).max(rect.top() + 12.),
                ),
                Vec2::new(w, h),
            );
            painter.image(
                tex.id(),
                area,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                if speaking {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(185, 190, 200)
                },
            );
        }
    }
    pub(super) fn save_browser(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new("存档与读取").open(&mut open).default_width(650.).default_height(580.).show(ctx,|ui|{
            ui.label("存档保存在当前 Windows 用户目录。手动存档后会自动生成画面缩略图。");
            egui::ScrollArea::vertical().show(ui,|ui|{for slot in["auto","slot-1","slot-2","slot-3"]{ui.push_id(slot,|ui|{
                let dir=save_directory(&self.project.id);let file=dir.join(format!("{slot}.json"));let save:Option<SaveGame>=std::fs::read(&file).ok().and_then(|b|serde_json::from_slice(&b).ok());
                egui::Frame::new().fill(BASE).inner_margin(12).corner_radius(6).show(ui,|ui|{ui.horizontal(|ui|{
                    let preview=dir.join(format!("{slot}.png"));if preview.is_file(){let revision=std::fs::metadata(&preview).and_then(|m|m.modified()).ok();let key=format!("save:{}:{revision:?}",preview.display());if !self.textures.contains_key(&key){let tex=image::open(&preview).map(|i|{let rgba=i.into_rgba8();ctx.load_texture(&key,egui::ColorImage::from_rgba_unmultiplied([rgba.width()as usize,rgba.height()as usize],rgba.as_raw()),egui::TextureOptions::LINEAR)}).map_err(|e|e.to_string());self.textures.insert(key.clone(),tex);}
if let Some(Ok(tex))=self.textures.get(&key){ui.add(egui::Image::new((tex.id(),Vec2::new(180.,110.))).fit_to_exact_size(Vec2::new(180.,110.)));}}
                    ui.vertical(|ui|{ui.label(RichText::new(if slot=="auto"{"自动存档"}else{slot}).color(ACCENT));if let Some(save)=&save{ui.label(&save.node_title);ui.label(RichText::new(format!("经过 {} 段 · {}",save.runtime.history.len(),save_age(save.saved_at))).small().color(MUTED));}else{ui.label("空白存档");}
                        ui.horizontal(|ui|{if slot!="auto"&&ui.add_enabled(self.runtime.is_some(),egui::Button::new(if save.is_some(){"覆盖此格"}else{"保存到此格"})).clicked(){self.save_slot(slot);}
if ui.add_enabled(save.is_some(),egui::Button::new("读取")).clicked(){self.load_slot(slot);}});
                    });
                });});
            });}});
        });
        self.saves_open &= open;
    }
    pub(super) fn graph_ports(
        &mut self,
        ui: &mut egui::Ui,
        canvas: Rect,
        nodes: &opengalgame::nodes::Nodes,
        zoom: f32,
    ) {
        if self.runtime.is_some() {
            return;
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        let mut connection = None;
        let visible = canvas.intersect(ui.clip_rect());
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        // Capture the press at the port, independently of overlapping scroll/node
        // widgets. This also handles a complete drag arriving within one frame.
        if let Some(origin) = pressed.filter(|p| visible.contains(*p)) {
            for node in nodes {
                let count = match node.kind {
                    NodeKind::Dialogue => 1,
                    NodeKind::Choice => node.choices.len(),
                    NodeKind::Ending => 0,
                };
                for index in 0..count {
                    let output = canvas.min
                        + Vec2::new(
                            node.position[0] + 222.,
                            node.position[1] + 32. + index as f32 * 24.,
                        ) * zoom;
                    if origin.distance(output) <= 12. {
                        self.advanced.link_drag = Some(Port {
                            node: node.id.clone(),
                            choice: (node.kind == NodeKind::Choice).then_some(index),
                        });
                        self.advanced.link_active = false;
                    }
                }
            }
        }
        for node in nodes {
            let node_rect = Rect::from_min_size(
                canvas.min + Vec2::new(node.position[0], node.position[1]) * zoom,
                Vec2::new(222., (56. + node.choices.len() as f32 * 24.).max(105.)) * zoom,
            );
            if !node_rect.expand(16.).intersects(visible) {
                continue;
            }
            let input = canvas.min + Vec2::new(node.position[0], node.position[1] + 48.) * zoom;
            let r = ui.interact(
                Rect::from_center_size(input, Vec2::splat(24.)),
                ui.id().with(("input", &node.id)),
                Sense::click(),
            );
            r.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("连接到 {}", node.title),
                )
            });
            ui.painter().circle_filled(
                input,
                6.,
                if self.advanced.link_drag.is_some() {
                    GOOD
                } else {
                    MUTED
                },
            );
            if let Some(port) = &self.advanced.link_drag
                && (r.clicked() || (released && pointer.is_some_and(|p| r.rect.contains(p))))
            {
                connection = Some((port.clone(), node.id.clone()));
            }
            let count = match node.kind {
                NodeKind::Dialogue => 1,
                NodeKind::Choice => node.choices.len(),
                NodeKind::Ending => 0,
            };
            for index in 0..count {
                let output = canvas.min
                    + Vec2::new(
                        node.position[0] + 222.,
                        node.position[1] + 32. + index as f32 * 24.,
                    ) * zoom;
                let port = Port {
                    node: node.id.clone(),
                    choice: (node.kind == NodeKind::Choice).then_some(index),
                };
                let r = ui.interact(
                    Rect::from_center_size(output, Vec2::splat(24.)),
                    ui.id().with(("output", &node.id, index)),
                    Sense::click_and_drag(),
                );
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("{} 的输出 {}", node.title, index + 1),
                    )
                });
                ui.painter().circle_filled(output, 6., ACCENT);
                if r.drag_started() {
                    self.advanced.link_drag = Some(port.clone());
                    self.advanced.link_active = true;
                }
                if r.clicked() {
                    self.advanced.link_drag = Some(port.clone());
                    self.advanced.link_active = false;
                }
                if self
                    .advanced
                    .link_drag
                    .as_ref()
                    .is_some_and(|p| p.node == node.id && p.choice == port.choice)
                    && let Some(pointer) = pointer
                {
                    ui.painter()
                        .line_segment([output, pointer], Stroke::new(2.0_f32, GOOD));
                }
            }
        }
        if let Some(port) = &self.advanced.link_drag
            && let Some(pos) = pointer
        {
            if ui.input(|i| i.pointer.primary_down())
                && let Some(node) = nodes.iter().find(|n| n.id == port.node)
            {
                let output = canvas.min
                    + Vec2::new(
                        node.position[0] + 222.,
                        node.position[1] + 32. + port.choice.unwrap_or(0) as f32 * 24.,
                    ) * zoom;
                self.advanced.link_active |= pos.distance(output) > 12.;
            }
            if released
                && visible.contains(pos)
                && let Some(target) = nodes.iter().find(|n| {
                    let input = canvas.min + Vec2::new(n.position[0], n.position[1] + 48.) * zoom;
                    pos.distance(input) <= 12.
                })
            {
                connection = Some((port.clone(), target.id.clone()));
            }
        }
        if let Some((port, target)) = connection {
            let result = authoring::connect(&mut self.project, &port, &target)
                .map(|_| "连线已更新，可撤销。");
            self.message(result);
            self.advanced.link_drag = None;
            self.advanced.link_active = false;
        } else if released && self.advanced.link_active {
            self.advanced.link_drag = None;
            self.advanced.link_active = false;
        }
        if self.advanced.link_drag.is_some() {
            ui.ctx().request_repaint();
        }
    }
    pub(super) fn remove_asset(&mut self, kind: &str, id: &str) {
        match kind {
            "character" => {
                self.project.characters.retain(|c| c.id != id);
                for n in &mut self.project.nodes {
                    if n.character.as_deref() == Some(id) {
                        n.character = None;
                        n.expression = None;
                    }
                    n.actors.retain(|a| a.character != id);
                }
            }
            "background" => {
                self.project.backgrounds.retain(|b| b.id != id);
                for n in &mut self.project.nodes {
                    if n.background.as_deref() == Some(id) {
                        n.background = None;
                    }
                }
            }
            "audio" => {
                self.project.audio.retain(|a| a.id != id);
                if self.project.presentation.bgm.as_deref() == Some(id) {
                    self.project.presentation.bgm = None;
                }
                for n in &mut self.project.nodes {
                    if n.bgm.as_deref() == Some(id) {
                        n.bgm = None;
                    }
                    if n.sound.as_deref() == Some(id) {
                        n.sound = None;
                    }
                }
            }
            _ => {}
        }
        self.message(Ok("素材已从项目移除，引用已清理；可使用撤销恢复。"));
    }
    pub(super) fn character_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("人物与表情库");
        ui.label("为人物导入默认立绘和表情；引用次数可以帮助判断替换与移除的影响。");
        if ui.button("+ 添加人物").clicked() {
            let id = (1..)
                .map(|i| format!("character_{i}"))
                .find(|id| self.project.characters.iter().all(|c| &c.id != id))
                .unwrap();
            self.project.characters.push(Character {
                id,
                name: "新人物".into(),
                color: [180, 180, 240],
                portrait: None,
                expressions: Default::default(),
                tags: String::new(),
            });
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for index in 0..self.project.characters.len() {
                ui.push_id(index, |ui| {
                    egui::Frame::new()
                        .fill(PANEL)
                        .inner_margin(18)
                        .corner_radius(7)
                        .show(ui, |ui| {
                            let c = self.project.characters[index].clone();
                            let uses = self
                                .project
                                .nodes
                                .iter()
                                .filter(|n| {
                                    n.character.as_deref() == Some(&c.id)
                                        || n.actors.iter().any(|a| a.character == c.id)
                                })
                                .count();
                            ui.horizontal_top(|ui| {
                                if let Some(file) = &c.portrait
                                    && let Some(tex) = self.texture(ui.ctx(), file)
                                {
                                    ui.add(
                                        egui::Image::new((tex.id(), Vec2::new(120., 170.)))
                                            .fit_to_exact_size(Vec2::new(120., 170.)),
                                    );
                                }
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!("{} · {} 个场景引用", c.id, uses))
                                            .small()
                                            .color(MUTED),
                                    );
                                    field_label(ui, "人物姓名");
                                    ui.text_edit_singleline(
                                        &mut self.project.characters[index].name,
                                    );
                                    field_label(ui, "标签（可自由填写）");
                                    ui.text_edit_singleline(
                                        &mut self.project.characters[index].tags,
                                    );
                                    ui.horizontal(|ui| {
                                        ui.label("姓名颜色");
                                        ui.color_edit_button_srgb(
                                            &mut self.project.characters[index].color,
                                        );
                                    });
                                    if ui.button("导入 / 替换默认立绘").clicked()
                                        && let Some(file) = self.asset_dialog()
                                    {
                                        self.project.characters[index].portrait = Some(file);
                                    }
                                    if ui.button(RichText::new("移除人物…").color(BAD)).clicked()
                                    {
                                        self.advanced.delete_asset =
                                            Some(("character".into(), c.id.clone()));
                                    }
                                });
                            });
                            egui::CollapsingHeader::new(format!(
                                "表情组（{}）",
                                c.expressions.len()
                            ))
                            .show(ui, |ui| {
                                let mut remove = None;
                                for (name, file) in &c.expressions {
                                    ui.push_id(name, |ui| {
                                        ui.horizontal(|ui| {
                                            if let Some(tex) = self.texture(ui.ctx(), file) {
                                                ui.add(
                                                    egui::Image::new((
                                                        tex.id(),
                                                        Vec2::new(46., 62.),
                                                    ))
                                                    .fit_to_exact_size(Vec2::new(46., 62.)),
                                                );
                                            }
                                            ui.label(name);
                                            if ui.button("替换").clicked()
                                                && let Some(file) = self.asset_dialog()
                                            {
                                                self.project.characters[index]
                                                    .expressions
                                                    .insert(name.clone(), file);
                                            }
                                            if ui.button("移除表情").clicked() {
                                                remove = Some(name.clone());
                                            }
                                        });
                                    });
                                }
                                if let Some(name) = remove {
                                    self.project.characters[index].expressions.remove(&name);
                                    for n in &mut self.project.nodes {
                                        if n.character.as_deref() == Some(&c.id)
                                            && n.expression.as_deref() == Some(&name)
                                        {
                                            n.expression = None;
                                        }
                                        for a in &mut n.actors {
                                            if a.character == c.id
                                                && a.expression.as_deref() == Some(&name)
                                            {
                                                a.expression = None;
                                            }
                                        }
                                    }
                                }
                                ui.label("新增表情时使用图片文件名作为表情名，同名会替换。");
                                if ui.button("+ 导入表情图片").clicked()
                                    && let Some(source) = rfd::FileDialog::new()
                                        .add_filter("表情图片", &["png", "jpg", "webp"])
                                        .pick_file()
                                {
                                    let name = source
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                    match import_asset(self.base(), &source) {
                                        Ok(file) => {
                                            self.project.characters[index]
                                                .expressions
                                                .insert(name, file);
                                        }
                                        Err(e) => self.message(Err::<String, _>(e)),
                                    }
                                }
                            });
                        });
                });
            }
        });
    }
    pub(super) fn background_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("背景缩略图库");
        if ui.button("+ 导入背景").clicked()
            && let Some(file) = self.asset_dialog()
        {
            let id = (1..)
                .map(|i| format!("background_{i}"))
                .find(|id| self.project.backgrounds.iter().all(|b| &b.id != id))
                .unwrap();
            self.project.backgrounds.push(Background {
                id,
                name: "新背景".into(),
                file,
                tags: String::new(),
            });
        }
        self.advanced.background_page = self
            .advanced
            .background_page
            .min(self.project.backgrounds.len().saturating_sub(1) / 18);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.advanced.background_page > 0,
                    egui::Button::new("上一页"),
                )
                .clicked()
            {
                self.advanced.background_page -= 1;
            }
            ui.label(format!("第 {} 页", self.advanced.background_page + 1));
            if ui
                .add_enabled(
                    (self.advanced.background_page + 1) * 18 < self.project.backgrounds.len(),
                    egui::Button::new("下一页"),
                )
                .clicked()
            {
                self.advanced.background_page += 1;
            }
        });
        let columns = (ui.available_width() / 300.).floor().max(1.) as usize;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("background_grid")
                .num_columns(columns)
                .spacing([16., 16.])
                .show(ui, |ui| {
                    for i in self.advanced.background_page * 18
                        ..((self.advanced.background_page + 1) * 18)
                            .min(self.project.backgrounds.len())
                    {
                        ui.push_id(i, |ui| {
                            egui::Frame::new()
                                .fill(PANEL)
                                .inner_margin(14)
                                .corner_radius(7)
                                .show(ui, |ui| {
                                    ui.set_width(260.);
                                    let b = self.project.backgrounds[i].clone();
                                    if let Some(tex) = self.thumbnail(ui.ctx(), &b.file) {
                                        ui.add(
                                            egui::Image::new((tex.id(), Vec2::new(260., 146.)))
                                                .fit_to_exact_size(Vec2::new(260., 146.)),
                                        );
                                    }
                                    ui.text_edit_singleline(&mut self.project.backgrounds[i].name);
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.project.backgrounds[i].tags,
                                        )
                                        .hint_text("标签：海湾、夜晚、雨天"),
                                    );
                                    let count = self
                                        .project
                                        .nodes
                                        .iter()
                                        .filter(|n| n.background.as_deref() == Some(&b.id))
                                        .count();
                                    ui.label(
                                        RichText::new(format!("{} · {} 个场景", b.id, count))
                                            .small()
                                            .color(MUTED),
                                    );
                                    ui.horizontal(|ui| {
                                        if ui.button("替换图片").clicked()
                                            && let Some(file) = self.asset_dialog()
                                        {
                                            self.project.backgrounds[i].file = file;
                                        }
                                        if ui.button("移除…").clicked() {
                                            self.advanced.delete_asset =
                                                Some(("background".into(), b.id));
                                        }
                                    });
                                });
                        });
                        if (i + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
    }
    pub(super) fn audio_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("音乐与音效");
        ui.label("导入 WAV、MP3、OGG 或 FLAC；在节点中指定背景音乐与进入音效。");
        if ui.button("+ 导入音轨").clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter("音频", &["wav", "mp3", "ogg", "flac"])
                .pick_file()
        {
            match import_audio(self.base(), &file) {
                Ok(relative) => {
                    let id = (1..)
                        .map(|i| format!("audio_{i}"))
                        .find(|id| self.project.audio.iter().all(|a| &a.id != id))
                        .unwrap();
                    self.project.audio.push(AudioAsset {
                        id,
                        name: file
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        file: relative,
                    });
                }
                Err(e) => self.message(Err::<String, _>(e)),
            }
        }
        if ui.button("停止试听").clicked() {
            self.advanced.sound.stop();
            self.advanced.preview_audio = false;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..self.project.audio.len() {
                let asset = self.project.audio[i].clone();
                ui.push_id(i, |ui| {
                    egui::Frame::new()
                        .fill(PANEL)
                        .inner_margin(14)
                        .corner_radius(7)
                        .show(ui, |ui| {
                            ui.text_edit_singleline(&mut self.project.audio[i].name);
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} 个场景引用",
                                    asset.id,
                                    self.project.asset_use_count(&asset.id)
                                ))
                                .small()
                                .color(MUTED),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("试听").clicked() {
                                    let result = safe_asset(self.base(), &asset.file)
                                        .and_then(|p| self.advanced.sound.effect(&p, 0.6));
                                    self.advanced.preview_audio = true;
                                    self.message(result.map(|_| "正在试听，可点击停止试听。"));
                                }
                                if ui.button("替换音频").clicked()
                                    && let Some(file) = rfd::FileDialog::new()
                                        .add_filter("音频", &["wav", "mp3", "ogg", "flac"])
                                        .pick_file()
                                {
                                    match import_audio(self.base(), &file) {
                                        Ok(relative) => self.project.audio[i].file = relative,
                                        Err(e) => self.message(Err::<String, _>(e)),
                                    }
                                }
                                if ui.button("移除…").clicked() {
                                    self.advanced.delete_asset =
                                        Some(("audio".into(), asset.id.clone()));
                                }
                            });
                        });
                });
            }
        });
    }
    pub(super) fn title_ui(&mut self, ui: &mut egui::Ui) {
        if self.advanced.sound.error.is_none()
            && let Some(file) = self
                .project
                .presentation
                .bgm
                .as_ref()
                .and_then(|id| self.project.audio.iter().find(|a| &a.id == id))
                .map(|a| a.file.clone())
        {
            let result = safe_asset(self.base(), &file).and_then(|path| {
                self.advanced
                    .sound
                    .music(Some(&path), self.advanced.settings.music)
            });
            if let Err(e) = result {
                self.advanced.sound.error = Some(e.to_string());
            }
        }
        let rect = ui.max_rect();
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0, BASE);
        let cover = self
            .project
            .presentation
            .cover
            .clone()
            .or_else(|| self.project.backgrounds.first().map(|b| b.file.clone()));
        if let Some(file) = cover
            && let Some(texture) = self.texture(ui.ctx(), &file)
        {
            paint_cover(&p, rect, &texture);
        }
        p.rect_filled(rect, 0, Color32::from_black_alpha(135));
        let accent = Color32::from_rgb(
            self.project.presentation.accent[0],
            self.project.presentation.accent[1],
            self.project.presentation.accent[2],
        );
        let panel = Rect::from_min_size(
            rect.min + Vec2::new(rect.width() * 0.09, rect.height() * 0.18),
            Vec2::new((rect.width() * 0.65).min(650.), rect.height() * 0.75),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(panel));
        child.label(
            RichText::new(&self.project.title)
                .size(46.)
                .strong()
                .color(INK),
        );
        child.label(RichText::new(&self.project.author).color(accent));
        child.add_space(35.);
        if child
            .add_sized([250., 46.], egui::Button::new("开始新的故事").fill(accent))
            .clicked()
        {
            self.runtime = Some(Runtime::new(&self.project));
            self.advanced.title_screen = false;
            self.advanced.scene_key.clear();
        }
        let auto = save_directory(&self.project.id).join("auto.json");
        if child
            .add_enabled(
                auto.is_file(),
                egui::Button::new("继续上次的故事").min_size(Vec2::new(250., 42.)),
            )
            .clicked()
        {
            self.load_slot("auto");
            if self.runtime.is_some() {
                self.advanced.title_screen = false;
            }
        }
        if child
            .add_sized([250., 42.], egui::Button::new("存档与读取"))
            .clicked()
        {
            self.saves_open = true;
        }
        if child
            .add_sized([250., 42.], egui::Button::new("阅读与声音设置"))
            .clicked()
        {
            self.advanced.settings_open = true;
        }
        if child
            .add_sized([250., 42.], egui::Button::new("结局收藏"))
            .clicked()
        {
            self.advanced.collection_open = true;
        }
        child.add_space(25.);
        child.label(RichText::new("点击按钮开始 · 支持键盘和鼠标").color(INK));
    }
    fn player_windows(&mut self, ctx: &egui::Context) {
        if self.advanced.settings_open {
            let mut open = true;
            egui::Window::new("阅读与声音设置")
                .open(&mut open)
                .default_width(480.)
                .show(ctx, |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.advanced.settings.font, 16.0..=36.0)
                            .text("正文字号"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.advanced.settings.cps, 0.0..=80.0)
                            .text("打字机速度，0=关闭"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.advanced.settings.music, 0.0..=1.0)
                            .text("背景音乐音量"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.advanced.settings.effects, 0.0..=1.0)
                            .text("音效音量"),
                    );
                    ui.checkbox(&mut self.advanced.settings.reduced_motion, "减少动态效果");
                    if let Some(error) = &self.advanced.sound.error {
                        ui.colored_label(BAD, error);
                        if ui.button("重试音频设备").clicked() {
                            self.advanced.sound.error = None;
                        }
                    }
                    self.advanced
                        .sound
                        .volumes(self.advanced.settings.music, self.advanced.settings.effects);
                    self.text_size = self.advanced.settings.font;
                    if ui.button("保存设置").clicked() {
                        self.persist_profile();
                        self.advanced.settings_open = false;
                    }
                });
            self.advanced.settings_open &= open;
        }
        if self.advanced.collection_open {
            let mut open = true;
            egui::Window::new("结局收藏")
                .open(&mut open)
                .default_width(580.)
                .show(ctx, |ui| {
                    let endings: Vec<_> = self
                        .project
                        .nodes
                        .iter()
                        .filter(|n| n.kind == NodeKind::Ending)
                        .cloned()
                        .collect();
                    ui.label(format!(
                        "已解锁 {} / {} 个结局",
                        self.advanced.unlocked.len(),
                        endings.len()
                    ));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for n in endings {
                            egui::CollapsingHeader::new(
                                if self.advanced.unlocked.contains(&n.id) {
                                    n.title
                                } else {
                                    "尚未抵达的结局".into()
                                },
                            )
                            .id_salt(&n.id)
                            .show(ui, |ui| {
                                if self.advanced.unlocked.contains(&n.id) {
                                    ui.label(n.text);
                                } else {
                                    ui.label("沿不同选择，找到属于你的结局。");
                                }
                            });
                        }
                    });
                });
            self.advanced.collection_open = open;
        }
    }
    pub(super) fn advanced_tick(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
                && let Some(slot) = user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<String>())
                && ["auto", "slot-1", "slot-2", "slot-3"].contains(&slot.as_str())
            {
                let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(png) =
                    image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                {
                    let preview = image::DynamicImage::ImageRgba8(png).thumbnail(360, 220);
                    let _ =
                        preview.save(save_directory(&self.project.id).join(format!("{slot}.png")));
                }
            }
        }
        if let Some((slot, delay)) = self.advanced.capture.take() {
            if delay > 0 {
                self.advanced.capture = Some((slot, delay - 1));
                ctx.request_repaint();
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(slot)));
            }
        }
        self.maintain_recovery(ctx);
        if self.runtime.is_none() && !self.advanced.title_screen && !self.advanced.preview_audio {
            self.advanced.sound.stop();
        }
        if let Some(r) = &self.runtime
            && self
                .project
                .node(&r.current.node)
                .is_some_and(|n| n.kind == NodeKind::Ending)
            && self.advanced.unlocked.insert(r.current.node.clone())
        {
            self.persist_profile();
        }
        if let Some(node) = self
            .runtime
            .as_ref()
            .and_then(|r| self.project.node(&r.current.node))
            .cloned()
        {
            self.prepare_scene(&node, true, ctx);
        }
    }
    pub(super) fn persist_profile(&self) {
        let _ = write_json(
            &save_directory(&self.project.id).join("profile.json"),
            &Profile {
                settings: self.advanced.settings.clone(),
                unlocked: self.advanced.unlocked.clone(),
            },
        );
    }
    pub(super) fn advanced_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.runtime.is_none(), egui::Button::new("批量导入对白"))
                .clicked()
            {
                self.advanced.bulk_open = true;
            }
            if ui
                .add_enabled(self.runtime.is_none(), egui::Button::new("全文替换"))
                .clicked()
            {
                self.advanced.replace_open = true;
            }
            if ui
                .selectable_label(self.advanced.focus, "专注写作")
                .clicked()
            {
                self.advanced.focus = !self.advanced.focus;
                self.section = Section::Script;
            }
            ui.menu_button("最近项目", |ui| {
                let files = self.advanced.recent.clone();
                if files.is_empty() {
                    ui.label("还没有最近打开的项目。");
                }
                for path in files {
                    if ui.button(path.display().to_string()).clicked() {
                        ui.close();
                        self.request(Pending::Open(path));
                    }
                }
            });
        });
    }
    pub(super) fn focus_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.selected > 0, egui::Button::new("上一段"))
                .clicked()
            {
                self.select(self.selected - 1);
            }
            if ui
                .add_enabled(
                    self.selected + 1 < self.project.nodes.len(),
                    egui::Button::new("下一段"),
                )
                .clicked()
            {
                self.select(self.selected + 1);
            }
            ui.label(format!(
                "第 {} / {} 段",
                self.selected + 1,
                self.project.nodes.len()
            ));
            if ui.button("返回完整工作区").clicked() {
                self.advanced.focus = false;
            }
        });
        ui.separator();
        let chars: Vec<_> = self
            .project
            .characters
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        if let Some(node) = self.project.nodes.get_mut(self.selected) {
            let width = ui.available_width().min(940.);
            let margin = (ui.available_width() - width) / 2.;
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.add_space(16.);
                    ui.label(RichText::new("专注写作").size(26.).strong());
                    ui.horizontal(|ui| {
                        ui.label("章节");
                        ui.text_edit_singleline(&mut node.chapter);
                        optional_combo(ui, "focus_speaker", &mut node.character, &chars, "旁白");
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut node.title)
                            .font(FontId::proportional(21.))
                            .desired_width(width)
                            .margin(12),
                    );
                    egui::ScrollArea::vertical()
                        .max_height((ui.available_height() - 50.).max(180.))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut node.text)
                                    .font(FontId::proportional(22.))
                                    .desired_width(width)
                                    .desired_rows(20)
                                    .margin(18),
                            );
                        });
                    ui.label(format!(
                        "{} 字 · Ctrl+S 保存 · Ctrl+Z 撤销",
                        node.text.chars().count()
                    ));
                });
            });
        }
    }
    pub(super) fn chapter_filter(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("chapters")
            .width(ui.available_width() - 12.)
            .selected_text(if self.advanced.chapter_filter.is_empty() {
                "全部章节"
            } else {
                &self.advanced.chapter_filter
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.advanced.chapter_filter, String::new(), "全部章节");
                for chapter in authoring::chapters(&self.project) {
                    ui.selectable_value(
                        &mut self.advanced.chapter_filter,
                        chapter.clone(),
                        &chapter,
                    );
                }
            });
    }
    pub(super) fn scene_options(&mut self, ui: &mut egui::Ui) {
        let chars: Vec<_> = self
            .project
            .characters
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        let audio: Vec<_> = self
            .project
            .audio
            .iter()
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect();
        let node = &mut self.project.nodes[self.selected];
        section_label(ui, "章节与演出");
        field_label(ui, "所属章节");
        ui.text_edit_singleline(&mut node.chapter);
        if let Some(c) = node
            .character
            .as_ref()
            .and_then(|id| self.project.characters.iter().find(|c| &c.id == id))
        {
            let expressions: Vec<_> = c
                .expressions
                .keys()
                .map(|s| (s.clone(), s.clone()))
                .collect();
            field_label(ui, "发言人表情");
            optional_combo(
                ui,
                "speaker_expression",
                &mut node.expression,
                &expressions,
                "默认立绘",
            );
        }
        ui.horizontal(|ui| {
            ui.label("转场");
            for (value, label) in [
                (Transition::None, "直接切换"),
                (Transition::Fade, "淡出 / 淡入"),
                (Transition::Slide, "滑入"),
            ] {
                ui.selectable_value(&mut node.transition, value, label);
            }
        });
        field_label(ui, "背景音乐（空则使用项目音乐）");
        let mut music = audio.clone();
        music.push(("$stop".into(), "停止音乐".into()));
        optional_combo(ui, "bgm", &mut node.bgm, &music, "使用项目音乐");
        field_label(ui, "进入场景时的音效");
        optional_combo(ui, "sound", &mut node.sound, &audio, "无音效");
        egui::CollapsingHeader::new("同屏人物").show(ui, |ui| {
            ui.label(
                RichText::new("发言人默认显示在舞台；这里可以额外添加其他人物。")
                    .small()
                    .color(MUTED),
            );
            let mut remove = None;
            for (i, actor) in node.actors.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    target_combo(ui, "actor", &mut actor.character, &chars);
                    let expressions: Vec<_> = self
                        .project
                        .characters
                        .iter()
                        .find(|c| c.id == actor.character)
                        .map(|c| {
                            c.expressions
                                .keys()
                                .map(|s| (s.clone(), s.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    optional_combo(
                        ui,
                        "expression",
                        &mut actor.expression,
                        &expressions,
                        "默认表情",
                    );
                    ui.horizontal(|ui| {
                        for side in [
                            PortraitSide::Left,
                            PortraitSide::Center,
                            PortraitSide::Right,
                        ] {
                            ui.selectable_value(&mut actor.side, side, side.label());
                        }
                    });
                    ui.add(egui::Slider::new(&mut actor.scale, 0.4..=1.6).text("大小"));
                    if ui.small_button("移除同屏人物").clicked() {
                        remove = Some(i);
                    }
                    ui.separator();
                });
            }
            if let Some(i) = remove {
                node.actors.remove(i);
            }
            if !chars.is_empty() && ui.button("+ 添加同屏人物").clicked() {
                node.actors.push(Actor {
                    character: chars[0].0.clone(),
                    expression: None,
                    side: PortraitSide::Left,
                    scale: 1.,
                });
            }
        });
    }
    pub(super) fn presentation_options(&mut self, ui: &mut egui::Ui) {
        section_label(ui, "游戏标题画面与图标");
        if ui.button("选择标题背景图片").clicked()
            && let Some(file) = self.asset_dialog()
        {
            self.project.presentation.cover = Some(file);
        }
        if ui.button("选择游戏图标（PNG / JPG）").clicked()
            && let Some(file) = self.asset_dialog()
        {
            self.project.presentation.icon = Some(file);
        }
        for (name, file) in [
            ("标题背景", self.project.presentation.cover.clone()),
            ("游戏图标", self.project.presentation.icon.clone()),
        ] {
            if let Some(file) = file {
                ui.label(format!("{name}：{file}"));
                if let Some(tex) = self.texture(ui.ctx(), &file) {
                    ui.add(
                        egui::Image::new((tex.id(), Vec2::new(130., 80.)))
                            .fit_to_exact_size(Vec2::new(130., 80.)),
                    );
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("游戏主题色");
            ui.color_edit_button_srgb(&mut self.project.presentation.accent);
        });
        ui.add(
            egui::Slider::new(&mut self.project.presentation.font_size, 16.0..=36.0)
                .text("默认正文字号"),
        );
        ui.add(
            egui::Slider::new(&mut self.project.presentation.typewriter_cps, 0.0..=80.0)
                .text("打字机速度（字/秒，0=关闭）"),
        );
        ui.add(
            egui::Slider::new(&mut self.project.presentation.transition_seconds, 0.0..=2.0)
                .text("转场时长（秒）"),
        );
        let audio: Vec<_> = self
            .project
            .audio
            .iter()
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect();
        field_label(ui, "项目默认音乐");
        optional_combo(
            ui,
            "default_music",
            &mut self.project.presentation.bgm,
            &audio,
            "无背景音乐",
        );
    }
    pub(super) fn advanced_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(snapshot) = self.advanced.recovery.clone() {
            egui::Modal::new(egui::Id::new("recovery_prompt")).show(ctx, |ui| {
                ui.set_width(510.);
                ui.heading("发现未保存的工程恢复副本");
                ui.label(snapshot.path.display().to_string());
                if snapshot.disk_fingerprint != fingerprint(&self.saved) {
                    ui.colored_label(BAD, "磁盘项目也有更新，恢复后请先另存为新项目核对内容。");
                }
                ui.label("恢复只加载到编辑器，不会立即覆盖磁盘文件。");
                ui.horizontal(|ui| {
                    if ui.button("恢复未保存内容").clicked() {
                        self.project = snapshot.project;
                        self.select(0);
                        self.advanced.recovery = None;
                        self.undo.push(self.saved.clone());
                        self.skip_history = true;
                        self.message(Ok("已恢复，请核对后保存或另存。"));
                    }
                    if ui.button("使用磁盘版本").clicked() {
                        let _ = recovery::dismiss(&self.path, &self.saved);
                        self.advanced.recovery = None;
                    }
                });
            });
        }
        if self.advanced.bulk_open {
            let mut open = true;
            egui::Window::new("批量导入剧本").open(&mut open).default_size([670.,560.]).show(ctx,|ui|{
            ui.label("每行一段对白：人物名：内容；旁白可直接写。用 [章节名] 分章，用 @背景：名称 切换背景。新人物会自动创建。");
            ui.horizontal(|ui|{ui.label("默认章节");ui.text_edit_singleline(&mut self.advanced.chapter);});
            egui::ScrollArea::vertical().max_height(350.).show(ui,|ui|{ui.add(egui::TextEdit::multiline(&mut self.advanced.bulk).desired_width(f32::INFINITY).desired_rows(16).hint_text("[第一章]\n乔安：你终于来了。\n林澈：我一直记得这个约定。"));});
            if ui.button("插入到当前节点之后").clicked(){match authoring::import_dialogue(&mut self.project,&self.advanced.bulk,self.selected,&self.advanced.chapter){Ok(count)=>{self.message(Ok(format!("已导入 {count} 段对白，可在分支图检查连接。")));self.advanced.bulk_open=false;},Err(e)=>self.message(Err::<String,_>(e))}}
        });
            self.advanced.bulk_open &= open;
        }
        if self.advanced.replace_open {
            let mut open = true;
            egui::Window::new("查找与批量替换")
                .open(&mut open)
                .default_width(570.)
                .show(ctx, |ui| {
                    field_label(ui, "查找文本（搜索标题、正文和选项，不修改节点标识）");
                    ui.text_edit_singleline(&mut self.advanced.find);
                    field_label(ui, "替换为");
                    ui.text_edit_singleline(&mut self.advanced.replacement);
                    let matches: Vec<_> = self
                        .project
                        .nodes
                        .iter()
                        .filter(|n| {
                            authoring::matches(n, &self.advanced.find)
                                && !self.advanced.find.is_empty()
                        })
                        .map(|n| (n.id.clone(), n.title.clone()))
                        .collect();
                    ui.label(format!("匹配 {} 个节点", matches.len()));
                    egui::ScrollArea::vertical()
                        .max_height(200.)
                        .show(ui, |ui| {
                            for (id, title) in matches {
                                if ui.button(title).clicked()
                                    && let Some(i) =
                                        self.project.nodes.iter().position(|n| n.id == id)
                                {
                                    self.select(i);
                                    self.section = Section::Script;
                                }
                            }
                        });
                    if ui
                        .add_enabled(
                            !self.advanced.find.is_empty(),
                            egui::Button::new("替换全部（可撤销）"),
                        )
                        .clicked()
                    {
                        let count = authoring::replace_all(
                            &mut self.project,
                            &self.advanced.find,
                            &self.advanced.replacement,
                        );
                        self.message(Ok(format!("已替换 {count} 处文本，可使用撤销恢复。")));
                    }
                });
            self.advanced.replace_open &= open;
        }
        if let Some(port) = self.advanced.edge.clone() {
            let mut open = true;
            egui::Window::new("分支连线设置")
                .open(&mut open)
                .default_width(440.)
                .show(ctx, |ui| {
                    let targets: Vec<_> = self
                        .project
                        .nodes
                        .iter()
                        .map(|n| (n.id.clone(), n.title.clone()))
                        .collect();
                    let flags: Vec<_> = self.project.flags.keys().cloned().collect();
                    if let Some(n) = self.project.nodes.iter_mut().find(|n| n.id == port.node) {
                        ui.label(&n.title);
                        if let Some(i) = port.choice {
                            if let Some(c) = n.choices.get_mut(i) {
                                ui.text_edit_singleline(&mut c.text);
                                target_combo(ui, "wire_target", &mut c.target, &targets);
                                flag_control(ui, "显示条件", &mut c.condition, &flags);
                                flag_control(ui, "选择后赋值", &mut c.set_flag, &flags);
                                if ui.button("断开连线").clicked() {
                                    c.target.clear();
                                }
                            }
                        } else {
                            optional_combo(ui, "wire_target", &mut n.next, &targets, "未连接");
                            if ui.button("断开连线").clicked() {
                                n.next = None;
                            }
                        }
                    }
                });
            if !open {
                self.advanced.edge = None;
            }
        }
        if let Some((kind, id)) = self.advanced.delete_asset.clone() {
            egui::Modal::new(egui::Id::new("asset_delete")).show(ctx, |ui| {
                ui.set_width(460.);
                ui.heading("移除项目素材？");
                ui.label(format!(
                    "{} 个场景引用了该素材，移除时会解除引用。磁盘原文件保留，可撤销。",
                    self.project.asset_use_count(&id)
                ));
                ui.horizontal(|ui| {
                    if ui.button("移除并解除引用").clicked() {
                        self.remove_asset(&kind, &id);
                        self.advanced.delete_asset = None;
                    }
                    if ui.button("取消").clicked() {
                        self.advanced.delete_asset = None;
                    }
                });
            });
        }
        self.player_windows(ctx);
    }
}
