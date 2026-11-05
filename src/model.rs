use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Component, Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    pub start: String,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub backgrounds: Vec<Background>,
    #[serde(default)]
    pub flags: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, crate::logic::Value>,
    pub nodes: crate::nodes::Nodes,
    #[serde(default)]
    pub audio: Vec<AudioAsset>,
    #[serde(default)]
    pub presentation: Presentation,
    #[serde(default)]
    pub tasks: Vec<CreativeTask>,
    #[serde(default)]
    pub save_compatibility: crate::compatibility::SavePolicy,
    #[serde(default, skip_serializing_if = "crate::production::is_default")]
    pub production: crate::production::Production,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Todo,
    Doing,
    Done,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreativeTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub status: TaskStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
    pub portrait: Option<String>,
    #[serde(default)]
    pub expressions: BTreeMap<String, String>,
    #[serde(default)]
    pub tags: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioAsset {
    pub id: String,
    pub name: String,
    pub file: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Presentation {
    pub cover: Option<String>,
    pub icon: Option<String>,
    pub accent: [u8; 3],
    pub font_size: f32,
    pub typewriter_cps: f32,
    pub transition_seconds: f32,
    pub bgm: Option<String>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            cover: None,
            icon: None,
            accent: [169, 156, 241],
            font_size: 22.,
            typewriter_cps: 0.,
            transition_seconds: 0.35,
            bgm: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    None,
    Fade,
    Slide,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Actor {
    pub character: String,
    pub expression: Option<String>,
    pub side: PortraitSide,
    pub scale: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Background {
    pub id: String,
    pub name: String,
    pub file: String,
    #[serde(default)]
    pub tags: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    #[default]
    Dialogue,
    Choice,
    Ending,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PortraitSide {
    Left,
    Center,
    #[default]
    Right,
}
impl PortraitSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "左侧",
            Self::Center => "中央",
            Self::Right => "右侧",
        }
    }
}
fn one() -> f32 {
    1.0
}
impl NodeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dialogue => "对白",
            Self::Choice => "选择",
            Self::Ending => "结局",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub position: [f32; 2],
    #[serde(default)]
    pub portrait_side: PortraitSide,
    #[serde(default = "one")]
    pub portrait_scale: f32,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub actors: Vec<Actor>,
    #[serde(default)]
    pub transition: Transition,
    #[serde(default)]
    pub bgm: Option<String>,
    #[serde(default)]
    pub sound: Option<String>,
    #[serde(default, skip_serializing_if = "crate::production::is_default")]
    pub direction: crate::production::Direction,
}
impl Node {
    pub fn new(id: String, kind: NodeKind, index: usize) -> Self {
        Self {
            title: format!("新{}", kind.label()),
            id,
            kind,
            text: "在这里写下故事。".into(),
            character: None,
            background: None,
            next: None,
            choices: vec![],
            position: [(index % 3) as f32 * 270., (index / 3) as f32 * 170.],
            portrait_side: PortraitSide::Right,
            portrait_scale: 1.0,
            chapter: "第一章".into(),
            expression: None,
            actors: vec![],
            transition: Transition::None,
            bgm: None,
            sound: None,
            direction: Default::default(),
        }
    }
    pub fn targets(&self) -> Vec<&str> {
        match self.kind {
            NodeKind::Dialogue => self.next.iter().map(String::as_str).collect(),
            NodeKind::Choice => self.choices.iter().map(|c| c.target.as_str()).collect(),
            NodeKind::Ending => vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Choice {
    pub text: String,
    pub target: String,
    #[serde(default)]
    pub condition: Option<FlagValue>,
    #[serde(default)]
    pub set_flag: Option<FlagValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<crate::logic::Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<crate::logic::Effect>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FlagValue {
    pub flag: String,
    pub value: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Clone, Debug)]
pub struct Issue {
    pub severity: Severity,
    pub node: Option<String>,
    pub message: String,
}
impl Project {
    pub fn uses_production(&self) -> bool {
        self.production != Default::default()
            || self.nodes.iter().any(|n| n.direction != Default::default())
    }
    pub fn uses_typed_logic(&self) -> bool {
        !self.variables.is_empty()
            || self.nodes.iter().any(|n| {
                n.choices
                    .iter()
                    .any(|c| c.when.is_some() || !c.effects.is_empty())
            })
    }
    pub fn image_files(&self) -> Vec<&str> {
        self.backgrounds
            .iter()
            .map(|b| b.file.as_str())
            .chain(self.characters.iter().flat_map(|c| {
                c.portrait
                    .iter()
                    .map(String::as_str)
                    .chain(c.expressions.values().map(String::as_str))
            }))
            .chain(self.presentation.cover.iter().map(String::as_str))
            .chain(self.presentation.icon.iter().map(String::as_str))
            .collect()
    }
    pub fn asset_use_count(&self, id: &str) -> usize {
        self.nodes
            .iter()
            .filter(|n| {
                n.character.as_deref() == Some(id)
                    || n.background.as_deref() == Some(id)
                    || n.bgm.as_deref() == Some(id)
                    || n.sound.as_deref() == Some(id)
                    || n.direction.voice.as_deref() == Some(id)
                    || n.direction
                        .audio
                        .iter()
                        .any(|a| a.audio.as_deref() == Some(id))
                    || n.direction.actions.iter().any(|a| match a {
                        crate::production::StageAction::Show { character, .. }
                        | crate::production::StageAction::Hide { character, .. }
                        | crate::production::StageAction::Move { character, .. }
                        | crate::production::StageAction::Expression { character, .. } => {
                            character == id
                        }
                        crate::production::StageAction::Background { background, .. } => {
                            background == id
                        }
                        _ => false,
                    })
                    || n.actors.iter().any(|a| a.character == id)
            })
            .count()
    }
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.find(id)
    }
    pub fn next_id(&self) -> String {
        (1..)
            .map(|n| format!("scene_{n}"))
            .find(|id| {
                self.node(id).is_none() && !self.save_compatibility.node_aliases.contains_key(id)
            })
            .unwrap()
    }
    pub fn rename_node(&mut self, old: &str, new: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!new.trim().is_empty(), "节点标识不能为空");
        anyhow::ensure!(old == new || self.node(new).is_none(), "节点标识已存在");
        anyhow::ensure!(self.node(old).is_some(), "原节点不存在");
        if old == new {
            return Ok(());
        }
        if self.start == old {
            self.start = new.into();
        }
        for i in 0..self.nodes.len() {
            let n = &self.nodes[i];
            if n.id != old
                && n.next.as_deref() != Some(old)
                && n.choices.iter().all(|c| c.target != old)
            {
                continue;
            }
            let n = &mut self.nodes[i];
            if n.id == old {
                n.id = new.into();
            }
            if n.next.as_deref() == Some(old) {
                n.next = Some(new.into());
            }
            for c in &mut n.choices {
                if c.target == old {
                    c.target = new.into();
                }
            }
        }
        for target in self.save_compatibility.node_aliases.values_mut() {
            if target == old {
                *target = new.into();
            }
        }
        self.save_compatibility.node_aliases.remove(new);
        self.save_compatibility
            .node_aliases
            .insert(old.into(), new.into());
        for case in &mut self.production.cases {
            if case.expected_node == old {
                case.expected_node = new.into();
            }
            for step in std::iter::once(&mut case.initial.current).chain(&mut case.initial.history)
            {
                if step.node == old {
                    step.node = new.into();
                }
            }
            for command in &mut case.commands {
                if let crate::debugger::Command::Jump { node } = command
                    && node == old
                {
                    *node = new.into();
                }
            }
        }
        Ok(())
    }
}
pub fn safe_asset(base: &Path, relative: &str) -> anyhow::Result<std::path::PathBuf> {
    let path = Path::new(relative);
    anyhow::ensure!(
        !relative.trim().is_empty() && !path.is_absolute(),
        "素材必须使用相对路径"
    );
    anyhow::ensure!(
        path.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "素材路径不能跨出项目目录"
    );
    // Check both separators, including Windows paths inspected on another OS.
    anyhow::ensure!(
        !relative.contains(':') && !relative.split(['/', '\\']).any(|p| p == ".."),
        "素材路径包含非法部分"
    );
    let root = base.canonicalize()?;
    let full = root.join(path).canonicalize()?;
    anyhow::ensure!(
        full.starts_with(&root) && full.is_file(),
        "素材必须是项目目录内的文件"
    );
    Ok(full)
}
pub fn validate(p: &Project, base: Option<&Path>) -> Vec<Issue> {
    let mut issues = vec![];
    let mut push = |severity, node: Option<&str>, message: String| {
        issues.push(Issue {
            severity,
            node: node.map(str::to_owned),
            message,
        })
    };
    if ![1, 2, 3].contains(&p.format_version) {
        push(Severity::Error, None, "不支持的项目格式版本".into());
    }
    if p.id.trim().is_empty() || p.title.trim().is_empty() {
        push(Severity::Error, None, "项目标识和标题不能为空".into());
    }
    if p.nodes.is_empty() {
        push(Severity::Error, None, "至少需要一个剧情节点".into());
    }
    if p.uses_production() && p.format_version < 3 {
        push(
            Severity::Error,
            None,
            "演出、配音、主题和节点赋值需要项目格式 3".into(),
        );
    }
    if let Some(theme) = &p.production.theme
        && let Err(e) = theme.validate()
    {
        push(Severity::Error, None, e.to_string());
    }
    if let Some(base) = base
        && let Some(file) = p
            .production
            .theme
            .as_ref()
            .and_then(|t| t.font_file.as_ref())
    {
        let result = safe_asset(base, file).and_then(|path| {
            let bytes = std::fs::read(path)?;
            anyhow::ensure!(bytes.len() <= 30 * 1024 * 1024, "字体超过 30 MB");
            ab_glyph::FontRef::try_from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("字体无法解析"))?;
            Ok(())
        });
        if let Err(e) = result {
            push(Severity::Error, None, format!("游戏字体：{e}"));
        }
    }
    if p.production.cases.len() > 256 {
        push(Severity::Error, None, "最多保存 256 个测试用例".into());
    }
    let has_logic = !p.variables.is_empty()
        || p.nodes.iter().any(|n| {
            n.choices
                .iter()
                .any(|c| c.when.is_some() || !c.effects.is_empty())
        });
    if has_logic && p.format_version < 2 {
        push(
            Severity::Error,
            None,
            "新增变量和复合条件需要项目格式 2，请用 0.4 保存".into(),
        );
    }
    for (key, value) in &p.variables {
        if key.trim().is_empty() || p.flags.contains_key(key) {
            push(
                Severity::Error,
                None,
                format!("变量标识为空或与旧布尔变量重复：{key}"),
            );
        }
        if let crate::logic::Value::Text(v) = value
            && v.len() > 65536
        {
            push(Severity::Error, None, format!("文本变量 {key} 超过 64 KiB"));
        }
    }
    let mut ids = HashSet::new();
    for n in &p.nodes {
        if n.id.trim().is_empty() || !ids.insert(n.id.as_str()) {
            push(Severity::Error, Some(&n.id), "节点标识为空或重复".into());
        }
    }
    if p.node(&p.start).is_none() {
        push(
            Severity::Error,
            None,
            format!("起始节点 {} 不存在", p.start),
        );
    }
    for old in p.save_compatibility.node_aliases.keys() {
        if p.node(old).is_some() {
            push(
                Severity::Error,
                None,
                format!("迁移规则的旧标识 {old} 仍被现有节点占用"),
            );
        } else if let Err(e) = crate::compatibility::resolve(p, old) {
            push(Severity::Error, None, format!("存档迁移规则错误：{e}"));
        }
    }
    let mut char_ids = HashSet::new();
    for c in &p.characters {
        if c.id.trim().is_empty() || !char_ids.insert(c.id.as_str()) {
            push(Severity::Error, None, "人物标识为空或重复".into());
        }
    }
    let mut bg_ids = HashSet::new();
    for b in &p.backgrounds {
        if b.id.trim().is_empty() || !bg_ids.insert(b.id.as_str()) {
            push(Severity::Error, None, "背景标识为空或重复".into());
        }
    }
    let mut audio_ids = HashSet::new();
    for a in &p.audio {
        if a.id.trim().is_empty() || !audio_ids.insert(a.id.as_str()) {
            push(Severity::Error, None, "音轨标识为空或重复".into());
        }
    }
    if let Some(id) = &p.presentation.bgm
        && !audio_ids.contains(id.as_str())
    {
        push(Severity::Error, None, format!("标题音乐 {id} 不存在"));
    }
    if !(12.0..=48.0).contains(&p.presentation.font_size)
        || !(0.0..=120.0).contains(&p.presentation.typewriter_cps)
        || !(0.0..=3.0).contains(&p.presentation.transition_seconds)
    {
        push(Severity::Error, None, "显示参数超出有效范围".into());
    }
    for n in &p.nodes {
        if let Err(e) = crate::production::validate_direction(p, n) {
            push(Severity::Error, Some(&n.id), e.to_string());
        }
        for audio in [&n.bgm, &n.sound].into_iter().flatten() {
            if audio != "$stop" && !audio_ids.contains(audio.as_str()) {
                push(Severity::Error, Some(&n.id), format!("音轨 {audio} 不存在"));
            }
        }
        if let (Some(id), Some(expression)) = (&n.character, &n.expression)
            && !p
                .characters
                .iter()
                .any(|c| &c.id == id && c.expressions.contains_key(expression))
        {
            push(Severity::Error, Some(&n.id), "发言人的表情不存在".into());
        }
        for a in &n.actors {
            if !p.characters.iter().any(|c| {
                c.id == a.character
                    && a.expression
                        .as_ref()
                        .is_none_or(|e| c.expressions.contains_key(e))
            }) {
                push(Severity::Error, Some(&n.id), "同屏人物或表情不存在".into());
            }
            if !a.scale.is_finite() || !(0.4..=1.6).contains(&a.scale) {
                push(Severity::Error, Some(&n.id), "同屏立绘大小无效".into());
            }
        }
        if n.text.trim().is_empty() {
            push(Severity::Warning, Some(&n.id), "正文为空".into());
        }
        if n.character
            .as_ref()
            .is_some_and(|id| !char_ids.contains(id.as_str()))
        {
            push(Severity::Error, Some(&n.id), "引用了不存在的人物".into());
        }
        if n.background
            .as_ref()
            .is_some_and(|id| !bg_ids.contains(id.as_str()))
        {
            push(Severity::Error, Some(&n.id), "引用了不存在的背景".into());
        }
        if n.kind == NodeKind::Dialogue && n.next.is_none() {
            push(
                Severity::Error,
                Some(&n.id),
                "对白需要指定下一节点，终点请使用结局类型".into(),
            );
        }
        if n.kind == NodeKind::Choice && n.choices.is_empty() {
            push(
                Severity::Error,
                Some(&n.id),
                "选择节点至少需要一个选项".into(),
            );
        }
        for t in n.targets() {
            if p.node(t).is_none() {
                push(Severity::Error, Some(&n.id), format!("目标节点 {t} 不存在"));
            }
        }
        for c in &n.choices {
            if let Some(condition) = &c.when
                && let Err(e) = condition.validate(p)
            {
                push(Severity::Error, Some(&n.id), format!("条件错误：{e}"));
            }
            for effect in &c.effects {
                if let Err(e) = effect.validate(p) {
                    push(Severity::Error, Some(&n.id), format!("赋值错误：{e}"));
                }
            }
            if c.text.trim().is_empty() {
                push(Severity::Error, Some(&n.id), "选项文字不能为空".into());
            }
            for f in [&c.condition, &c.set_flag].into_iter().flatten() {
                if !p.flags.contains_key(&f.flag) {
                    push(
                        Severity::Error,
                        Some(&n.id),
                        format!("变量 {} 未定义", f.flag),
                    );
                }
            }
        }
        if !n.position.iter().all(|v| v.is_finite()) {
            push(Severity::Error, Some(&n.id), "节点位置无效".into());
        }
        if !n.portrait_scale.is_finite() || !(0.4..=1.6).contains(&n.portrait_scale) {
            push(
                Severity::Error,
                Some(&n.id),
                "立绘缩放必须在 0.4–1.6 之间".into(),
            );
        }
    }
    if let Some(base) = base {
        for file in p.image_files() {
            match safe_asset(base, file).and_then(|full| {
                let (w, h) = image::ImageReader::open(full)?
                    .with_guessed_format()?
                    .into_dimensions()?;
                anyhow::ensure!(
                    w > 0 && h > 0 && w <= 8192 && h <= 8192,
                    "图片尺寸必须在 1–8192 像素内"
                );
                Ok(())
            }) {
                Ok(()) => {}
                Err(e) => push(Severity::Error, None, format!("素材 {file}：{e}")),
            }
        }
        for a in &p.audio {
            if let Err(e) = safe_asset(base, &a.file).and_then(|full| {
                rodio::Decoder::try_from(std::fs::File::open(full)?)
                    .map(|_| ())
                    .map_err(Into::into)
            }) {
                push(
                    Severity::Error,
                    None,
                    format!("音轨 {} 无法读取：{e}", a.name),
                );
            }
        }
    }
    let mut reached = HashSet::new();
    let mut queue = VecDeque::from([p.start.as_str()]);
    while let Some(id) = queue.pop_front() {
        if reached.insert(id)
            && let Some(n) = p.node(id)
        {
            queue.extend(n.targets());
        }
    }
    for n in &p.nodes {
        if !reached.contains(n.id.as_str()) {
            push(
                Severity::Warning,
                Some(&n.id),
                "从开头无法到达此节点".into(),
            );
        }
    }
    if !p
        .nodes
        .iter()
        .any(|n| n.kind == NodeKind::Ending && reached.contains(n.id.as_str()))
    {
        push(Severity::Error, None, "从开头无法到达任何结局".into());
    }
    // Bound state exploration using the exact runtime evaluator and transactional effects.
    if !issues.iter().any(|i| i.severity == Severity::Error) {
        let mut states = HashSet::new();
        let initial = crate::runtime::Runtime::new(p);
        if let Some(error) = &initial.error {
            issues.push(Issue {
                severity: Severity::Error,
                node: Some(p.start.clone()),
                message: error.clone(),
            });
        }
        let mut pending = VecDeque::from([initial.current]);
        while let Some(step) = pending.pop_front() {
            if states.len() >= 8192 {
                issues.push(Issue {
                    severity: Severity::Warning,
                    node: None,
                    message: "状态较多，条件路径检查在 8192 个状态后停止".into(),
                });
                break;
            }
            if !states.insert((
                step.node.clone(),
                step.flags.clone(),
                step.variables.clone(),
            )) {
                continue;
            }
            let Some(node) = p.node(&step.node) else {
                continue;
            };
            let mut runtime = crate::runtime::Runtime {
                current: step,
                history: vec![],
                error: None,
            };
            let choices = if node.kind == NodeKind::Choice {
                runtime
                    .visible_choices(p)
                    .into_iter()
                    .map(Some)
                    .collect::<Vec<_>>()
            } else if node.kind == NodeKind::Dialogue {
                vec![None]
            } else {
                vec![]
            };
            if node.kind == NodeKind::Choice && choices.is_empty() {
                issues.push(Issue {
                    severity: Severity::Error,
                    node: Some(node.id.clone()),
                    message: "存在到达此处后没有可选选项的变量状态".into(),
                });
            }
            for choice in choices {
                let before = runtime.clone();
                match runtime.advance(p, choice) {
                    Ok(()) => pending.push_back(runtime.current.clone()),
                    Err(e) => issues.push(Issue {
                        severity: Severity::Error,
                        node: Some(node.id.clone()),
                        message: format!("条件路径执行失败：{e}"),
                    }),
                }
                runtime = before;
            }
        }
    }
    issues
}

pub fn arrange_nodes(project: &mut Project) {
    let mut ranks = BTreeMap::new();
    let mut pending = VecDeque::from([(project.start.clone(), 0usize)]);
    while let Some((id, rank)) = pending.pop_front() {
        if ranks.contains_key(&id) {
            continue;
        }
        if let Some(node) = project.node(&id) {
            ranks.insert(id, rank);
            pending.extend(
                node.targets()
                    .into_iter()
                    .map(|id| (id.to_owned(), rank + 1)),
            );
        }
    }
    let last = ranks.values().max().copied().unwrap_or(0) + 1;
    let mut rows = BTreeMap::<usize, usize>::new();
    for node in &mut project.nodes {
        let rank = ranks.get(&node.id).copied().unwrap_or(last);
        let row = rows.entry(rank).or_default();
        node.position = [32. + rank as f32 * 285., 32. + *row as f32 * 170.];
        *row += 1;
    }
}
