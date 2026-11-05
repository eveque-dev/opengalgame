//! Saves follow stable node identities and visited control flow, not presentation.
use crate::{
    model::*,
    runtime::{Runtime, SaveGame},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavePolicy {
    #[serde(default)]
    pub node_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub legacy_baselines: Vec<std::sync::Arc<LegacyBaseline>>,
}
impl PartialEq for SavePolicy {
    fn eq(&self, other: &Self) -> bool {
        self.node_aliases == other.node_aliases
            && self.legacy_baselines.len() == other.legacy_baselines.len()
            && self
                .legacy_baselines
                .iter()
                .zip(&other.legacy_baselines)
                .all(|(a, b)| std::sync::Arc::ptr_eq(a, b) || **a == **b)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LegacyBaseline {
    pub fingerprint: String,
    pub nodes: BTreeMap<String, NodeSignature>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SaveCompatibility {
    pub nodes: BTreeMap<String, NodeSignature>,
    #[serde(default)]
    pub variables: BTreeMap<String, crate::logic::ValueKind>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NodeSignature {
    pub kind: NodeKind,
    pub next: Option<String>,
    pub choices: Vec<ChoiceSignature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter: Vec<crate::logic::Effect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit: Vec<crate::logic::Effect>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChoiceSignature {
    pub target: String,
    pub condition: Option<FlagValue>,
    pub set_flag: Option<FlagValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<crate::logic::Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<crate::logic::Effect>,
}
impl NodeSignature {
    pub fn of(node: &Node) -> Self {
        Self {
            kind: node.kind,
            enter: node.direction.enter.clone(),
            exit: node.direction.exit.clone(),
            next: if node.kind == NodeKind::Dialogue {
                node.next.clone()
            } else {
                None
            },
            choices: if node.kind == NodeKind::Choice {
                node.choices
                    .iter()
                    .map(|c| ChoiceSignature {
                        target: c.target.clone(),
                        condition: c.condition.clone(),
                        set_flag: c.set_flag.clone(),
                        when: c.when.clone(),
                        effects: c.effects.clone(),
                    })
                    .collect()
            } else {
                vec![]
            },
        }
    }
    fn mapped(&self, p: &Project) -> Result<Self> {
        let mut n = self.clone();
        if let Some(next) = &mut n.next {
            *next = resolve(p, next)?;
        }
        for c in &mut n.choices {
            c.target = resolve(p, &c.target)?;
        }
        Ok(n)
    }
}

// Preserve the exact serialized field order of the released 0.2 Project.
pub fn legacy_fingerprint(p: &Project) -> String {
    #[derive(Serialize)]
    struct Legacy<'a> {
        format_version: u32,
        id: &'a str,
        title: &'a str,
        author: &'a str,
        start: &'a str,
        characters: &'a Vec<Character>,
        backgrounds: &'a Vec<Background>,
        flags: &'a BTreeMap<String, bool>,
        nodes: &'a crate::nodes::Nodes,
        audio: &'a Vec<AudioAsset>,
        presentation: &'a Presentation,
    }
    let old = Legacy {
        format_version: p.format_version,
        id: &p.id,
        title: &p.title,
        author: &p.author,
        start: &p.start,
        characters: &p.characters,
        backgrounds: &p.backgrounds,
        flags: &p.flags,
        nodes: &p.nodes,
        audio: &p.audio,
        presentation: &p.presentation,
    };
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&old).expect("serializable project"))
    )
}
pub fn register_legacy_baseline(p: &mut Project, old: &Project) -> Result<()> {
    ensure!(p.id == old.id, "旧项目标识不同，不能作为该作品的存档基线");
    let fingerprint = legacy_fingerprint(old);
    if p.save_compatibility
        .legacy_baselines
        .iter()
        .any(|b| b.fingerprint == fingerprint)
    {
        return Ok(());
    }
    ensure!(
        p.save_compatibility.legacy_baselines.len() < 16,
        "兼容基线最多保留 16 个版本"
    );
    p.save_compatibility
        .legacy_baselines
        .push(std::sync::Arc::new(LegacyBaseline {
            fingerprint,
            nodes: old
                .nodes
                .iter()
                .map(|n| (n.id.clone(), NodeSignature::of(n)))
                .collect(),
        }));
    Ok(())
}
pub fn capture(p: &Project, r: &Runtime) -> SaveCompatibility {
    SaveCompatibility {
        variables: p
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.kind()))
            .collect(),
        nodes: std::iter::once(&r.current)
            .chain(&r.history)
            .filter_map(|s| {
                p.node(&s.node)
                    .map(|n| (s.node.clone(), NodeSignature::of(n)))
            })
            .collect(),
    }
}
pub fn resolve(p: &Project, id: &str) -> Result<String> {
    let mut result = id.to_owned();
    let mut seen = BTreeSet::new();
    while let Some(next) = p.save_compatibility.node_aliases.get(&result) {
        ensure!(seen.insert(result.clone()), "节点迁移规则存在循环：{id}");
        result = next.clone();
    }
    ensure!(
        p.node(&result).is_some(),
        "旧存档节点 {id} 没有可用的迁移目标"
    );
    Ok(result)
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct LoadReport {
    pub legacy: bool,
    pub content_updated: bool,
    pub migrated_nodes: usize,
    pub added_flags: Vec<String>,
}
impl LoadReport {
    pub fn description(&self) -> String {
        let mut parts = vec![if self.legacy {
            "已兼容读取 0.2 旧存档".to_owned()
        } else {
            "已读取进度".to_owned()
        }];
        if self.content_updated {
            parts.push("已适配当前版本内容".into());
        }
        if self.migrated_nodes > 0 {
            parts.push(format!("迁移 {} 个历史节点", self.migrated_nodes));
        }
        if !self.added_flags.is_empty() {
            parts.push(format!(
                "新增变量采用默认值：{}",
                self.added_flags.join("、")
            ));
        }
        parts.join("；")
    }
}
pub fn load(p: &Project, save: SaveGame) -> Result<(Runtime, LoadReport)> {
    ensure!(save.project_id == p.id, "此存档属于其他作品");
    ensure!([1, 2, 3, 4].contains(&save.version), "不支持的存档版本");
    ensure!(save.runtime.error.is_none(), "此存档包含无效运行状态");
    ensure!(save.runtime.history.len() <= 500, "存档回退历史超出上限");
    let mut report = LoadReport {
        legacy: save.version == 1,
        content_updated: save.fingerprint != crate::runtime::fingerprint(p),
        ..Default::default()
    };
    let exact_legacy = save.version == 1 && save.fingerprint == legacy_fingerprint(p);
    let signatures = if save.version >= 2 {
        Some(
            save.compatibility
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("存档缺少兼容信息"))?
                .nodes
                .clone(),
        )
    } else if exact_legacy {
        None
    } else {
        Some(
            p.save_compatibility
                .legacy_baselines
                .iter()
                .find(|b| b.fingerprint == save.fingerprint)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "无法确认旧存档对应的剧情；请在项目设置导入当时的旧项目作为兼容基线"
                    )
                })?
                .nodes
                .clone(),
        )
    };
    let schema = save
        .compatibility
        .as_ref()
        .map(|c| c.variables.clone())
        .unwrap_or_default();
    for (key, kind) in &schema {
        ensure!(
            p.variables.get(key).is_some_and(|v| v.kind() == *kind),
            "变量 {key} 已删除或类型已改变"
        );
    }
    let mut runtime = save.runtime;
    let mut migrated = BTreeSet::new();
    let mut added = BTreeSet::new();
    for step in std::iter::once(&mut runtime.current).chain(runtime.history.iter_mut()) {
        ensure!(
            step.variables.keys().all(|k| schema.contains_key(k)),
            "存档变量缺少类型声明"
        );
        ensure!(
            step.variables.iter().all(|(k, v)| p
                .variables
                .get(k)
                .is_some_and(|initial| initial.kind() == v.kind())),
            "存档包含未知或类型不符的变量"
        );
        for (key, initial) in &p.variables {
            if !step.variables.contains_key(key) {
                ensure!(!schema.contains_key(key), "存档缺少已有变量 {key}");
                step.variables.insert(key.clone(), initial.clone());
                added.insert(key.clone());
            }
        }
        let old = step.node.clone();
        let id = resolve(p, &old)?;
        if let Some(signatures) = &signatures {
            let signature = signatures
                .get(&old)
                .ok_or_else(|| anyhow::anyhow!("存档缺少节点 {old} 的兼容信息"))?;
            ensure!(
                signature.mapped(p)? == NodeSignature::of(p.node(&id).unwrap()),
                "节点 {old} 的类型、跳转或选择条件已改变；需要明确处理剧情迁移，不能直接读取"
            );
        }
        ensure!(
            step.flags.keys().all(|k| p.flags.contains_key(k)),
            "存档包含已删除或未知的变量"
        );
        for (key, value) in &p.flags {
            if !step.flags.contains_key(key) {
                step.flags.insert(key.clone(), *value);
                added.insert(key.clone());
            }
        }
        if old != id {
            migrated.insert(old);
        }
        step.node = id;
        for stage in [&mut step.stage, &mut step.scene_start]
            .into_iter()
            .flatten()
        {
            crate::assets::migrate_stage(p, stage);
            stage.validate(p)?;
        }
        if let Some(start) = &step.scene_start {
            let fresh =
                crate::production::StageState::entry(p, p.node(&step.node).unwrap(), Some(start));
            step.stage = Some(fresh.finished(p.node(&step.node).unwrap()));
            step.scene_start = Some(fresh);
        }
    }
    report.migrated_nodes = migrated.len();
    report.added_flags = added.into_iter().collect();
    Ok((runtime, report))
}
