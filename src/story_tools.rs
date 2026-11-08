//! Author context, validated branch drafts, and bounded consistency-check inputs.
use crate::model::{CharacterProfile, Choice, Node, NodeKind, Project};
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub const INPUT_LIMIT: usize = 48_000;
pub const AUDIT_MEMO_BYTES: usize = 3_600;
pub const BRANCH_SCENE_LIMIT: usize = 24;

#[derive(Default, Serialize)]
pub struct ContextPacket<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recap: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<ContextCharacter<'a>>,
}

#[derive(Serialize)]
pub struct ContextCharacter<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub profile: &'a CharacterProfile,
}

pub fn selected_context<'a>(
    project: &'a Project,
    include_recap: bool,
    cards: &BTreeSet<String>,
) -> ContextPacket<'a> {
    ContextPacket {
        recap: include_recap
            .then(|| project.story_recap.trim())
            .filter(|recap| !recap.is_empty()),
        characters: project
            .characters
            .iter()
            .filter(|character| cards.contains(&character.id))
            .map(|character| ContextCharacter {
                id: &character.id,
                name: &character.name,
                profile: &character.profile,
            })
            .collect(),
    }
}

pub fn decode_json<T: DeserializeOwned>(text: &str) -> Result<T> {
    let text = text.trim();
    let payload = text
        .strip_prefix("```")
        .and_then(|fenced| fenced.split_once('\n'))
        .and_then(|(_, rest)| rest.trim_end().strip_suffix("```"))
        .unwrap_or(text);
    serde_json::from_str(payload).map_err(|error| anyhow!("无法读取模型的结构化结果。{error}"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchDraft {
    pub entry: String,
    pub scenes: Vec<BranchScene>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchScene {
    pub id: String,
    pub title: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub choices: Vec<BranchChoice>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchChoice {
    pub text: String,
    pub target: String,
}

impl BranchScene {
    fn targets(&self) -> impl Iterator<Item = &str> {
        self.next
            .iter()
            .map(String::as_str)
            .chain(self.choices.iter().map(|choice| choice.target.as_str()))
    }
}

impl BranchDraft {
    pub fn validate(&self, project: &Project) -> Result<()> {
        ensure!(
            (2..=BRANCH_SCENE_LIMIT).contains(&self.scenes.len()),
            "分支草稿需要 2～{BRANCH_SCENE_LIMIT} 个节点。"
        );
        let mut scenes = HashMap::new();
        let mut incoming = HashMap::<&str, Vec<&str>>::new();
        let mut endings = Vec::new();
        let mut has_branch = false;
        for scene in &self.scenes {
            let id = scene.id.as_str();
            ensure!(
                !id.is_empty()
                    && id.len() <= 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                    && scenes.insert(id, scene).is_none(),
                "草稿节点标识无效或重复（{id}）。"
            );
            ensure!(
                valid_label(&scene.title, 100),
                "节点 {id} 的标题不能为空或超过 100 字。"
            );
            ensure!(scene.text.len() <= 24_000, "节点 {id} 的对白过长。");
            if let Some(character) = &scene.character {
                ensure!(
                    project.characters.iter().any(|c| &c.id == character),
                    "节点 {id} 引用了不存在的人物 {character}。"
                );
            }
            match scene.kind {
                NodeKind::Dialogue => ensure!(
                    scene.next.is_some() && scene.choices.is_empty(),
                    "对白 {id} 需要一个跳转目标。"
                ),
                NodeKind::Choice => {
                    ensure!(
                        scene.next.is_none() && (2..=6).contains(&scene.choices.len()),
                        "选择节点 {id} 需要 2～6 个选项。"
                    );
                    ensure!(
                        scene
                            .choices
                            .iter()
                            .all(|choice| valid_label(&choice.text, 120)),
                        "选择节点 {id} 的选项文字为空或超过 120 字。"
                    );
                    has_branch |= scene
                        .choices
                        .iter()
                        .any(|c| c.target != scene.choices[0].target);
                }
                NodeKind::Ending => {
                    ensure!(
                        scene.next.is_none() && scene.choices.is_empty(),
                        "结局 {id} 不能继续跳转。"
                    );
                    endings.push(id);
                }
            }
            for target in scene.targets() {
                incoming.entry(target).or_default().push(id);
            }
        }
        ensure!(scenes.contains_key(self.entry.as_str()), "草稿入口不存在。");
        ensure!(has_branch, "草稿需要至少一个通向不同节点的选择。");
        ensure!(!endings.is_empty(), "草稿至少需要一个结局。");
        for target in incoming.keys() {
            ensure!(
                scenes.contains_key(target),
                "跳转目标 {target} 不在草稿中。"
            );
        }
        let reachable = walk([self.entry.as_str()], |id| scenes[id].targets().collect());
        ensure!(
            reachable.len() == scenes.len(),
            "草稿中存在从入口无法抵达的节点。"
        );
        let escapable = walk(endings, |id| incoming.get(id).cloned().unwrap_or_default());
        ensure!(
            escapable.len() == scenes.len(),
            "草稿中有无法抵达任何结局的死路。"
        );
        Ok(())
    }

    /// Build the entire insertion before changing the project. Existing routes stay in place.
    pub fn insert(&self, project: &mut Project, attach: Option<(&str, &str)>) -> Result<String> {
        self.validate(project)?;
        let attach = attach
            .map(|(id, label)| {
                let index = project
                    .nodes
                    .iter()
                    .position(|node| node.id == id && node.kind == NodeKind::Choice)
                    .ok_or_else(|| anyhow!("请选中要新增选项的选择节点。"))?;
                let label = label.trim();
                ensure!(valid_label(label, 120), "请输入新选项文字（最多 120 字）。");
                ensure!(
                    project.nodes[index]
                        .choices
                        .iter()
                        .all(|c| c.text.trim() != label),
                    "当前节点已有同名选项。"
                );
                Ok((index, label))
            })
            .transpose()?;
        let mut occupied: HashSet<_> = project.nodes.iter().map(|node| node.id.clone()).collect();
        occupied.extend(project.save_compatibility.node_aliases.keys().cloned());
        let mut sequence = (1..).map(|i| format!("ai_scene_{i}"));
        let ids: HashMap<_, _> = self
            .scenes
            .iter()
            .map(|scene| {
                let id = sequence.find(|id| occupied.insert(id.clone())).unwrap();
                (scene.id.as_str(), id)
            })
            .collect();
        let entry = ids[self.entry.as_str()].clone();
        let offset = project
            .nodes
            .iter()
            .map(|node| node.position[1])
            .filter(|y| y.is_finite())
            .fold(0., f32::max)
            + 200.;
        let nodes: Vec<_> = self
            .scenes
            .iter()
            .enumerate()
            .map(|(index, scene)| {
                let mut node = Node::new(ids[scene.id.as_str()].clone(), scene.kind, index);
                node.position[1] += offset;
                node.chapter = "AI 分支草稿".into();
                node.title = scene.title.trim().to_owned();
                node.text = scene.text.clone();
                node.character = scene.character.clone();
                node.next = scene.next.as_ref().map(|next| ids[next.as_str()].clone());
                node.choices = scene
                    .choices
                    .iter()
                    .map(|choice| plain_choice(choice.text.trim(), &ids[choice.target.as_str()]))
                    .collect();
                node
            })
            .collect();
        if let Some((index, label)) = attach {
            project.nodes[index]
                .choices
                .push(plain_choice(label, &entry));
        }
        let end = project.nodes.len();
        project.nodes.splice(end..end, nodes);
        Ok(entry)
    }
}

fn valid_label(label: &str, max: usize) -> bool {
    let label = label.trim();
    !label.is_empty() && label.chars().count() <= max && !label.chars().any(char::is_control)
}

fn plain_choice(label: &str, target: &str) -> Choice {
    Choice {
        text: label.into(),
        target: target.into(),
        condition: None,
        set_flag: None,
        when: None,
        effects: vec![],
    }
}

fn walk<'a>(
    seeds: impl IntoIterator<Item = &'a str>,
    edges: impl Fn(&'a str) -> Vec<&'a str>,
) -> HashSet<&'a str> {
    let mut seen = HashSet::new();
    let mut pending: VecDeque<_> = seeds.into_iter().collect();
    while let Some(id) = pending.pop_front() {
        if seen.insert(id) {
            pending.extend(edges(id));
        }
    }
    seen
}

pub struct AuditPlan {
    header: String,
    parts: Vec<String>,
    pub node_ids: HashSet<String>,
}

impl AuditPlan {
    pub fn prepare(project: &Project, context: &ContextPacket<'_>) -> Result<Self> {
        ensure!(!project.nodes.is_empty(), "作品没有可检查的剧情节点。");
        let node_ids: HashSet<_> = project.nodes.iter().map(|n| n.id.clone()).collect();
        ensure!(
            node_ids.len() == project.nodes.len(),
            "请先修复重复的节点标识。"
        );
        let header = serde_json::to_string(&json!({
            "start": project.start,
            "cast": project.characters.iter().map(|c| json!({"id": c.id, "name": c.name})).collect::<Vec<_>>(),
            "context": context,
        }))?;
        ensure!(
            header.len() <= 15_000,
            "本次附加设定太长，请减少选中的人物卡或缩短前情摘要。"
        );
        let room = INPUT_LIMIT - header.len() - AUDIT_MEMO_BYTES - 500;
        let mut parts = vec![String::new()];
        let definitions = serde_json::to_string(&json!({
            "flags": project.flags, "variables": project.variables
        }))?;
        pack_record(&mut parts, "变量定义", &definitions, room);
        for node in &project.nodes {
            let record = serde_json::to_string(&json!({
                "id": node.id, "title": node.title, "chapter": node.chapter,
                "kind": node.kind, "character": node.character, "text": node.text,
                "next": node.next.as_ref().filter(|_| node.kind == NodeKind::Dialogue),
                "choices": if node.kind == NodeKind::Choice { node.choices.as_slice() } else { &[] },
                "enter": node.direction.enter, "exit": node.direction.exit
            }))?;
            pack_record(&mut parts, &node.id, &record, room);
        }
        Ok(Self {
            header,
            parts,
            node_ids,
        })
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn input(&self, index: usize, memo: &str) -> String {
        let memo = &memo[..memo.floor_char_boundary(AUDIT_MEMO_BYTES.min(memo.len()))];
        format!(
            "作品资料\n{}\n此前批次摘要\n{memo}\n剧本批次 {}/{}\n{}",
            self.header,
            index + 1,
            self.len(),
            self.parts[index]
        )
    }
}

fn pack_record(parts: &mut Vec<String>, label: &str, mut record: &str, room: usize) {
    if record.len() < room {
        if parts.last().unwrap().len() + record.len() + 1 > room {
            parts.push(String::new());
        }
        let part = parts.last_mut().unwrap();
        part.push_str(record);
        part.push('\n');
        return;
    }
    let label = &label[..label.floor_char_boundary(label.len().min(180))];
    let mut segment = 1;
    while !record.is_empty() {
        let prefix = format!("记录 {label} · 片段 {segment}\n");
        let end = record.floor_char_boundary((room - prefix.len() - 1).min(record.len()));
        let part = format!("{prefix}{}\n", &record[..end]);
        if parts.last().is_some_and(String::is_empty) {
            *parts.last_mut().unwrap() = part;
        } else {
            parts.push(part);
        }
        record = &record[end..];
        segment += 1;
    }
}

#[derive(Debug, Deserialize)]
pub struct AuditBatch {
    pub findings: Vec<AuditFinding>,
    #[serde(default)]
    pub memo: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditFinding {
    pub category: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub evidence: String,
    pub suggestion: String,
}

impl AuditFinding {
    pub fn is_locatable(&self, known: &HashSet<String>) -> bool {
        !self.nodes.is_empty()
            && self.nodes.len() <= 8
            && self.nodes.iter().all(|id| known.contains(id))
            && valid_label(&self.title, 200)
            && valid_label(&self.category, 40)
            && !self.evidence.trim().is_empty()
            && self.evidence.len() <= 6000
            && !self.suggestion.trim().is_empty()
            && self.suggestion.len() <= 6000
    }
}
