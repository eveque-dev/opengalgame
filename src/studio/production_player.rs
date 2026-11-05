use super::*;
use opengalgame::production::{self, Channel, GameTheme, StageState};
impl Studio {
    pub(super) fn prepare_production(
        &mut self,
        node: &Node,
        playing: bool,
        new_scene: bool,
        ctx: &egui::Context,
    ) {
        if new_scene {
            self.advanced.sound.stop_voice();
            self.advanced.sound.stop_effects();
            self.v5.voice_started = false;
            if playing && self.advanced.sound.error.is_none() {
                let mut effects = vec![];
                if let Some(id) = &node.sound {
                    effects.push((id.clone(), 1.));
                }
                for (id, gain) in effects {
                    if let Err(e) = self.play_effect(&id, gain) {
                        self.advanced.sound.error = Some(e.to_string());
                        break;
                    }
                }
                for cue in &node.direction.audio {
                    if cue.channel == Channel::Effects {
                        if let Some(id) = &cue.audio {
                            if let Err(e) = self.play_effect(id, cue.volume) {
                                self.advanced.sound.error = Some(e.to_string());
                                break;
                            }
                        } else {
                            self.advanced.sound.stop_effects();
                        }
                    }
                }
            }
        }
        let preview = self
            .v5
            .preview
            .as_ref()
            .filter(|(id, _)| id == &node.id)
            .map(|(_, t)| t.elapsed().as_secs_f32());
        let elapsed = if playing {
            if self.v5.instant_scene {
                f32::MAX
            } else {
                self.advanced.scene_at.elapsed().as_secs_f32()
            }
        } else {
            preview.unwrap_or(f32::MAX)
        };
        self.v5.frame = if self.project.uses_production() || preview.is_some() {
            let start = if playing {
                self.runtime
                    .as_ref()
                    .and_then(|r| r.current.scene_start.clone())
                    .unwrap_or_else(|| StageState::entry(&self.project, node, None))
            } else {
                StageState::entry(&self.project, node, None)
            };
            Some(production::sample(
                &start,
                node,
                elapsed,
                self.advanced.settings.reduced_motion,
            ))
        } else {
            None
        };
        self.v5.sequence_complete = self.v5.frame.as_ref().is_none_or(|f| f.complete);
        if !self.v5.sequence_complete {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if playing && self.advanced.sound.error.is_none() {
            let result = (|| -> anyhow::Result<()> {
                if let Some(stage) = self.runtime.as_ref().and_then(|r| r.current.stage.clone()) {
                    for (ambient, track, fade) in [
                        (false, stage.music, stage.music_fade),
                        (true, stage.ambience, stage.ambience_fade),
                    ] {
                        let path = track
                            .as_ref()
                            .map(|t| self.audio_file(&t.audio))
                            .transpose()?;
                        self.advanced.sound.track(
                            ambient,
                            path.as_deref(),
                            track.as_ref().map(|t| t.volume).unwrap_or(1.),
                            track.as_ref().is_none_or(|t| t.looped),
                            fade,
                        )?;
                    }
                } else {
                    let id = node
                        .bgm
                        .as_ref()
                        .or(self.project.presentation.bgm.as_ref())
                        .filter(|id| id.as_str() != "$stop");
                    let path = id.map(|id| self.audio_file(id)).transpose()?;
                    self.advanced
                        .sound
                        .track(false, path.as_deref(), 1., true, 0.)?;
                    self.advanced.sound.track(true, None, 1., true, 0.)?;
                }
                if !self.v5.voice_started
                    && self.v5.sequence_complete
                    && (self.v5.instant_scene
                        || self.advanced.scene_at.elapsed().as_secs_f32()
                            >= self.project.presentation.transition_seconds)
                {
                    self.v5.voice_started = true;
                    if let Some(id) = &node.direction.voice {
                        let path = self.audio_file(id)?;
                        self.advanced.sound.voice(&path)?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                self.advanced.sound.error = Some(e.to_string());
                self.status = "音频暂不可用，剧情仍可继续；可在声音设置重试。".into();
            }
        }
        self.advanced.sound.duck_voice = self.advanced.settings.duck_voice;
        self.advanced.sound.set_levels(
            self.advanced.settings.music,
            self.advanced.settings.ambience,
            self.advanced.settings.voice,
            self.advanced.settings.effects,
        );
        if self.advanced.sound.voice_playing() || self.advanced.sound.fading() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    pub(super) fn audio_file(&self, id: &str) -> anyhow::Result<PathBuf> {
        let asset = self
            .project
            .audio
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("音频 {id} 不存在"))?;
        safe_asset(self.base(), &asset.file)
    }
    fn play_effect(&mut self, id: &str, gain: f32) -> anyhow::Result<()> {
        let path = self.audio_file(id)?;
        self.advanced.sound.effect(&path, gain)
    }
    pub(super) fn replay_voice(&mut self, node_id: &str) {
        let id = self
            .project
            .node(node_id)
            .and_then(|n| n.direction.voice.clone());
        if let Some(id) = id {
            let result = self
                .audio_file(&id)
                .and_then(|path| self.advanced.sound.voice(&path))
                .map(|_| "正在重播配音");
            self.message(result);
        }
    }
    pub(super) fn dialogue_elapsed(&self, node: &Node) -> f32 {
        if self.v5.instant_scene {
            return f32::MAX;
        }
        (self.advanced.scene_at.elapsed().as_secs_f32()
            - node
                .direction
                .actions
                .iter()
                .map(|a| a.duration(self.advanced.settings.reduced_motion))
                .sum::<f32>())
        .max(0.)
    }
    pub(super) fn paint_directed_actor(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        dialogue_h: f32,
        actor: &production::RenderActor,
        speaking: bool,
    ) {
        let a = &actor.actor;
        let file = self
            .project
            .characters
            .iter()
            .find(|c| c.id == a.character)
            .and_then(|c| {
                a.expression
                    .as_ref()
                    .and_then(|e| c.expressions.get(e))
                    .or(c.portrait.as_ref())
            })
            .cloned();
        if let Some(file) = file
            && let Some(tex) = self.texture(ui.ctx(), &file)
        {
            let h = (rect.height() * 0.67 * a.scale).min(rect.height() - 30.);
            let w = h * tex.aspect_ratio();
            let x = rect.left() + 24. + (rect.width() - w - 48.) * actor.position;
            let y = (rect.bottom() - dialogue_h - h + 54.).max(rect.top() + 12.);
            let tint = if speaking { 255 } else { 190 };
            painter.image(
                tex.id(),
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                Color32::from_rgba_unmultiplied(
                    tint,
                    tint,
                    tint,
                    (actor.alpha.clamp(0., 1.) * 255.) as u8,
                ),
            );
        }
    }
    pub(super) fn ensure_game_font(&mut self, ctx: &egui::Context) {
        if let Some(family) = self.v5.pending_font.clone()
            && ctx.fonts(|f| f.families().contains(&family))
        {
            self.v5.font_family = family;
            self.v5.pending_font = None;
        }
        let theme = self.project.production.theme.clone().unwrap_or_default();
        let key = format!(
            "{}|{:?}|{}",
            theme.font_family,
            theme.font_file,
            self.path.display()
        );
        if self.v5.font_key == key {
            return;
        }
        self.v5.font_key = key;
        self.v5.pending_font = None;
        self.v5.font_family = egui::FontFamily::Proportional;
        if theme.font_family == "微软雅黑" && theme.font_file.is_none() {
            self.v5.font_family = egui::FontFamily::Proportional;
            return;
        }
        let file = if let Some(file) = &theme.font_file {
            safe_asset(self.base(), file).ok()
        } else {
            Some(PathBuf::from(if theme.font_family == "宋体" {
                r"C:\Windows\Fonts\simsun.ttc"
            } else {
                r"C:\Windows\Fonts\simhei.ttf"
            }))
        };
        if let Some(file) = file
            && let Ok(bytes) = std::fs::read(file)
            && ab_glyph::FontRef::try_from_slice(&bytes).is_ok()
        {
            use sha2::{Digest, Sha256};
            let name = format!("game-{:x}", Sha256::digest(&bytes));
            let family = egui::FontFamily::Name(name.clone().into());
            if ctx.fonts(|f| f.families().contains(&family)) {
                self.v5.font_family = family;
                return;
            }
            let mut fonts = ctx.fonts(|f| f.definitions().clone());
            if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
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
            }
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .insert(family.clone(), vec![name, "chinese".into()]);
            ctx.set_fonts(fonts);
            self.v5.pending_font = Some(family);
            ctx.request_repaint();
        } else {
            self.v5.font_family = egui::FontFamily::Proportional;
            self.status = "字体不可用，暂用系统中文字体。".into();
        }
    }
    pub(super) fn game_theme(&self) -> GameTheme {
        self.project.production.theme.clone().unwrap_or_default()
    }
    pub(super) fn game_style(&self, ui: &mut egui::Ui) {
        if self.project.production.theme.is_none() {
            return;
        }
        let theme = self.game_theme();
        let style = ui.style_mut();
        style.visuals.override_text_color = Some(Color32::from_rgb(
            theme.text[0],
            theme.text[1],
            theme.text[2],
        ));
        let fill = Color32::from_rgb(theme.choice[0], theme.choice[1], theme.choice[2]);
        style.visuals.widgets.inactive.bg_fill = fill;
        style.visuals.widgets.inactive.weak_bg_fill = fill;
        style.visuals.widgets.hovered.bg_fill = fill.gamma_multiply(1.25);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(theme.radius);
        style.override_font_id = Some(FontId::new(theme.choice_size, self.v5.font_family.clone()));
    }
}
