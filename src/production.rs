//! Serializable direction, player theme and deterministic stage sampling.
use crate::{logic::Effect, model::*};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Production {
    pub theme: Option<GameTheme>,
    pub cases: Vec<crate::testing::TestCase>,
    pub asset_aliases: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Direction {
    pub enter: Vec<Effect>,
    pub exit: Vec<Effect>,
    pub actions: Vec<StageAction>,
    pub voice: Option<String>,
    pub audio: Vec<AudioCue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageAction {
    Show {
        character: String,
        expression: Option<String>,
        side: PortraitSide,
        scale: f32,
        seconds: f32,
    },
    Hide {
        character: String,
        seconds: f32,
    },
    Move {
        character: String,
        side: PortraitSide,
        scale: f32,
        seconds: f32,
    },
    Expression {
        character: String,
        expression: Option<String>,
    },
    Wait {
        seconds: f32,
    },
    Background {
        background: String,
        seconds: f32,
    },
}
impl StageAction {
    pub fn seconds(&self) -> f32 {
        match self {
            Self::Show { seconds, .. }
            | Self::Hide { seconds, .. }
            | Self::Move { seconds, .. }
            | Self::Wait { seconds }
            | Self::Background { seconds, .. } => *seconds,
            Self::Expression { .. } => 0.,
        }
    }
    pub fn duration(&self, reduced: bool) -> f32 {
        if reduced && !matches!(self, Self::Wait { .. }) {
            0.
        } else {
            self.seconds()
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Show { .. } => "人物入场",
            Self::Hide { .. } => "人物退场",
            Self::Move { .. } => "移动与缩放",
            Self::Expression { .. } => "切换表情",
            Self::Wait { .. } => "等待",
            Self::Background { .. } => "切换背景",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    Music,
    Ambience,
    Effects,
}
impl Channel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Music => "音乐",
            Self::Ambience => "环境音",
            Self::Effects => "短音效",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioCue {
    pub channel: Channel,
    pub audio: Option<String>,
    pub volume: f32,
    pub looped: bool,
    pub fade: f32,
}
impl Default for AudioCue {
    fn default() -> Self {
        Self {
            channel: Channel::Music,
            audio: None,
            volume: 1.,
            looped: true,
            fade: 0.5,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub audio: String,
    pub volume: f32,
    pub looped: bool,
    pub fade: f32,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StageState {
    pub actors: Vec<Actor>,
    pub background: Option<String>,
    pub music: Option<Track>,
    pub ambience: Option<Track>,
    pub music_fade: f32,
    pub ambience_fade: f32,
}
impl StageState {
    pub fn validate(&self, p: &Project) -> Result<()> {
        ensure!(self.actors.len() <= 64, "存档同屏人物超过上限");
        for actor in &self.actors {
            ensure!(
                p.characters.iter().any(|c| c.id == actor.character
                    && actor
                        .expression
                        .as_ref()
                        .is_none_or(|e| c.expressions.contains_key(e))),
                "存档人物或表情不存在"
            );
            ensure!((0.4..=1.6).contains(&actor.scale), "存档立绘大小无效");
        }
        ensure!(
            self.background
                .as_ref()
                .is_none_or(|id| p.backgrounds.iter().any(|b| &b.id == id)),
            "存档背景不存在"
        );
        for track in [&self.music, &self.ambience].into_iter().flatten() {
            ensure!(
                p.audio.iter().any(|a| a.id == track.audio)
                    && (0.0..=1.0).contains(&track.volume)
                    && (0.0..=10.0).contains(&track.fade),
                "存档音轨参数无效"
            );
        }
        ensure!(
            (0.0..=10.0).contains(&self.music_fade) && (0.0..=10.0).contains(&self.ambience_fade),
            "存档音频渐变参数无效"
        );
        Ok(())
    }
    pub fn entry(p: &Project, node: &Node, previous: Option<&Self>) -> Self {
        let mut state = previous.cloned().unwrap_or_default();
        if previous.is_none() {
            state.music = p.presentation.bgm.as_ref().map(|audio| Track {
                audio: audio.clone(),
                volume: 1.,
                looped: true,
                fade: 0.,
            });
        }
        if let Some(background) = &node.background {
            state.background = Some(background.clone());
        }
        if !node.actors.is_empty() {
            state.actors = node.actors.clone();
        }
        if let Some(character) = &node.character {
            let actor = Actor {
                character: character.clone(),
                expression: node.expression.clone(),
                side: node.portrait_side,
                scale: node.portrait_scale,
            };
            if let Some(existing) = state.actors.iter_mut().find(|a| a.character == *character) {
                if node.expression.is_some() {
                    existing.expression = node.expression.clone();
                }
            } else {
                state.put(actor);
            }
        }
        if let Some(audio) = &node.bgm {
            state.music_fade = 0.;
            state.music = (audio != "$stop").then(|| Track {
                audio: audio.clone(),
                volume: 1.,
                looped: true,
                fade: 0.,
            });
        }
        for cue in &node.direction.audio {
            let track = cue.audio.as_ref().map(|id| Track {
                audio: id.clone(),
                volume: cue.volume,
                looped: cue.looped,
                fade: cue.fade,
            });
            match cue.channel {
                Channel::Music => {
                    state.music = track;
                    state.music_fade = cue.fade;
                }
                Channel::Ambience => {
                    state.ambience = track;
                    state.ambience_fade = cue.fade;
                }
                Channel::Effects => {}
            }
        }
        state
    }
    fn put(&mut self, actor: Actor) {
        if let Some(old) = self
            .actors
            .iter_mut()
            .find(|a| a.character == actor.character)
        {
            *old = actor;
        } else {
            self.actors.push(actor);
        }
    }
    pub fn apply(&mut self, action: &StageAction) {
        match action {
            StageAction::Show {
                character,
                expression,
                side,
                scale,
                ..
            } => self.put(Actor {
                character: character.clone(),
                expression: expression.clone(),
                side: *side,
                scale: *scale,
            }),
            StageAction::Hide { character, .. } => {
                self.actors.retain(|a| &a.character != character)
            }
            StageAction::Move {
                character,
                side,
                scale,
                ..
            } => {
                if let Some(actor) = self.actors.iter_mut().find(|a| &a.character == character) {
                    actor.side = *side;
                    actor.scale = *scale;
                }
            }
            StageAction::Expression {
                character,
                expression,
            } => {
                if let Some(actor) = self.actors.iter_mut().find(|a| &a.character == character) {
                    actor.expression = expression.clone();
                }
            }
            StageAction::Background { background, .. } => {
                self.background = Some(background.clone())
            }
            StageAction::Wait { .. } => {}
        }
    }
    pub fn finished(&self, node: &Node) -> Self {
        let mut stage = self.clone();
        for action in &node.direction.actions {
            stage.apply(action);
        }
        stage
    }
}
#[derive(Clone, Debug)]
pub struct RenderActor {
    pub actor: Actor,
    pub position: f32,
    pub alpha: f32,
}
#[derive(Clone)]
pub struct StageFrame {
    pub actors: Vec<RenderActor>,
    pub background: Option<String>,
    pub shade: f32,
    pub complete: bool,
}
pub fn position(side: PortraitSide) -> f32 {
    match side {
        PortraitSide::Left => 0.,
        PortraitSide::Center => 0.5,
        PortraitSide::Right => 1.,
    }
}
pub fn sample(start: &StageState, node: &Node, elapsed: f32, reduced: bool) -> StageFrame {
    let mut stage = start.clone();
    let mut remaining = elapsed.max(0.);
    let mut active = None;
    for action in &node.direction.actions {
        let duration = action.duration(reduced);
        if remaining >= duration {
            stage.apply(action);
            remaining -= duration;
        } else {
            active = Some((action, remaining / duration));
            break;
        }
    }
    let mut actors: Vec<_> = stage
        .actors
        .iter()
        .map(|actor| RenderActor {
            actor: actor.clone(),
            position: position(actor.side),
            alpha: 1.,
        })
        .collect();
    let mut shade = 0.;
    let mut background = stage.background;
    if let Some((action, t)) = active {
        let t = t * t * (3. - 2. * t);
        match action {
            StageAction::Show {
                character,
                expression,
                side,
                scale,
                ..
            } => {
                actors.retain(|a| &a.actor.character != character);
                actors.push(RenderActor {
                    actor: Actor {
                        character: character.clone(),
                        expression: expression.clone(),
                        side: *side,
                        scale: *scale,
                    },
                    position: position(*side),
                    alpha: t,
                });
            }
            StageAction::Hide { character, .. } => {
                if let Some(a) = actors.iter_mut().find(|a| &a.actor.character == character) {
                    a.alpha = 1. - t;
                }
            }
            StageAction::Move {
                character,
                side,
                scale,
                ..
            } => {
                if let Some(a) = actors.iter_mut().find(|a| &a.actor.character == character) {
                    a.position += (position(*side) - a.position) * t;
                    a.actor.scale += (*scale - a.actor.scale) * t;
                }
            }
            StageAction::Background {
                background: target, ..
            } => {
                shade = 1. - (2. * t - 1.).abs();
                if t >= 0.5 {
                    background = Some(target.clone());
                }
            }
            _ => {}
        }
    }
    StageFrame {
        actors,
        background,
        shade,
        complete: active.is_none(),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GameTheme {
    pub name: String,
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub name_color: [u8; 3],
    pub choice: [u8; 3],
    pub choice_text: [u8; 3],
    pub menu: [u8; 3],
    pub opacity: u8,
    pub panel_height: f32,
    pub panel_width: f32,
    pub radius: u8,
    pub button_height: f32,
    pub name_size: f32,
    pub choice_size: f32,
    pub name_box: bool,
    pub menu_position: PortraitSide,
    pub save_columns: usize,
    pub font_family: String,
    pub font_file: Option<String>,
}
impl Default for GameTheme {
    fn default() -> Self {
        Self {
            name: "海夜".into(),
            panel: [15, 24, 38],
            text: [230, 234, 243],
            name_color: [185, 164, 244],
            choice: [41, 54, 74],
            choice_text: [240, 241, 247],
            menu: [21, 29, 43],
            opacity: 240,
            panel_height: 0.36,
            panel_width: 0.96,
            radius: 8,
            button_height: 38.,
            name_size: 22.,
            choice_size: 18.,
            name_box: false,
            menu_position: PortraitSide::Left,
            save_columns: 1,
            font_family: "微软雅黑".into(),
            font_file: None,
        }
    }
}
impl GameTheme {
    pub fn preset(index: usize) -> Self {
        match index {
            1 => Self {
                name: "纸与墨".into(),
                panel: [245, 239, 225],
                text: [40, 40, 45],
                name_color: [86, 53, 68],
                choice: [221, 211, 192],
                choice_text: [40, 40, 45],
                menu: [230, 222, 207],
                opacity: 245,
                radius: 2,
                name_box: true,
                font_family: "宋体".into(),
                ..Default::default()
            },
            2 => Self {
                name: "樱色书笺".into(),
                panel: [44, 29, 45],
                name_color: [255, 186, 214],
                choice: [88, 54, 83],
                menu: [33, 24, 39],
                radius: 16,
                name_box: true,
                menu_position: PortraitSide::Center,
                save_columns: 2,
                ..Default::default()
            },
            _ => Self::default(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (0.25..=0.75).contains(&self.panel_height) && (0.6..=0.98).contains(&self.panel_width),
            "对话框比例无效"
        );
        ensure!(
            (28.0..=72.0).contains(&self.button_height)
                && (14.0..=40.0).contains(&self.name_size)
                && (14.0..=36.0).contains(&self.choice_size),
            "主题字号或按钮高度无效"
        );
        ensure!(
            (1..=3).contains(&self.save_columns) && self.radius <= 32,
            "主题存档列数或圆角无效"
        );
        ensure!(
            ["微软雅黑", "宋体", "黑体", "自定义"].contains(&self.font_family.as_str()),
            "字体选项无效"
        );
        ensure!(
            self.font_family != "自定义" || self.font_file.is_some(),
            "请导入自定义字体"
        );
        Ok(())
    }
}
pub fn validate_direction(p: &Project, node: &Node) -> Result<()> {
    let d = &node.direction;
    ensure!(
        d.actions.len() <= 128 && d.enter.len() <= 64 && d.exit.len() <= 64 && d.audio.len() <= 32,
        "演出动作或赋值超过上限"
    );
    for effect in d.enter.iter().chain(d.exit.iter()) {
        effect.validate(p)?;
    }
    let mut stage = StageState::entry(p, node, None);
    for action in &d.actions {
        ensure!(
            action.seconds().is_finite() && (0.0..=30.0).contains(&action.seconds()),
            "演出时长必须在 0～30 秒之间"
        );
        match action {
            StageAction::Show {
                character,
                expression,
                scale,
                ..
            } => {
                ensure!((0.4..=1.6).contains(scale), "立绘大小无效");
                let c = p
                    .characters
                    .iter()
                    .find(|c| &c.id == character)
                    .ok_or_else(|| anyhow::anyhow!("入场人物 {character} 不存在"))?;
                ensure!(
                    expression
                        .as_ref()
                        .is_none_or(|e| c.expressions.contains_key(e)),
                    "入场表情不存在"
                );
            }
            StageAction::Hide { character, .. }
            | StageAction::Move { character, .. }
            | StageAction::Expression { character, .. } => {
                ensure!(
                    p.characters.iter().any(|c| &c.id == character),
                    "演出人物 {character} 不存在"
                );
                if let StageAction::Move { scale, .. } = action {
                    ensure!((0.4..=1.6).contains(scale), "立绘大小无效");
                }
                if let StageAction::Expression {
                    expression: Some(e),
                    ..
                } = action
                {
                    ensure!(
                        p.characters
                            .iter()
                            .any(|c| &c.id == character && c.expressions.contains_key(e)),
                        "演出表情不存在"
                    );
                }
            }
            StageAction::Background { background, .. } => ensure!(
                p.backgrounds.iter().any(|b| &b.id == background),
                "演出背景不存在"
            ),
            _ => {}
        }
        stage.apply(action);
    }
    for id in d
        .voice
        .iter()
        .chain(d.audio.iter().filter_map(|a| a.audio.as_ref()))
    {
        ensure!(p.audio.iter().any(|a| &a.id == id), "演出音频 {id} 不存在");
    }
    for cue in &d.audio {
        ensure!(
            (0.0..=1.0).contains(&cue.volume) && (0.0..=10.0).contains(&cue.fade),
            "音频音量或渐变时长无效"
        );
        ensure!(
            cue.channel != Channel::Effects || !cue.looped,
            "短音效不能循环；持续声音请使用环境音通道"
        );
    }
    Ok(())
}
