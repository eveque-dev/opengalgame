use crate::model::{NodeKind, Project};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub node: String,
    pub flags: BTreeMap<String, bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Runtime {
    pub current: Step,
    pub history: Vec<Step>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub project_id: String,
    pub fingerprint: String,
    pub runtime: Runtime,
    #[serde(default)]
    pub saved_at: u128,
    #[serde(default)]
    pub node_title: String,
}
pub fn fingerprint(project: &Project) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(project).unwrap_or_default())
    )
}
impl Runtime {
    pub fn new(p: &Project) -> Self {
        Self {
            current: Step {
                node: p.start.clone(),
                flags: p.flags.clone(),
            },
            history: vec![],
        }
    }
    pub fn at(p: &Project, node: &str) -> Self {
        let mut r = Self::new(p);
        r.current.node = node.into();
        r
    }
    pub fn visible_choices(&self, p: &Project) -> Vec<usize> {
        p.node(&self.current.node)
            .filter(|n| n.kind == NodeKind::Choice)
            .map(|n| {
                n.choices
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        c.condition
                            .as_ref()
                            .is_none_or(|f| self.current.flags.get(&f.flag) == Some(&f.value))
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn advance(&mut self, p: &Project, choice: Option<usize>) -> Result<()> {
        let node = p
            .node(&self.current.node)
            .ok_or_else(|| anyhow::anyhow!("当前节点不存在"))?;
        let mut next = self.current.clone();
        match node.kind {
            NodeKind::Ending => anyhow::bail!("已到达结局"),
            NodeKind::Dialogue => {
                ensure!(choice.is_none(), "对白节点不接受选项");
                next.node = node
                    .next
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("没有下一节点"))?;
            }
            NodeKind::Choice => {
                let index = choice.ok_or_else(|| anyhow::anyhow!("请先选择选项"))?;
                ensure!(self.visible_choices(p).contains(&index), "此选项不可用");
                let c = &node.choices[index];
                next.node = c.target.clone();
                if let Some(f) = &c.set_flag {
                    ensure!(p.flags.contains_key(&f.flag), "变量不存在");
                    next.flags.insert(f.flag.clone(), f.value);
                }
            }
        }
        ensure!(p.node(&next.node).is_some(), "目标节点不存在");
        self.history.push(self.current.clone());
        if self.history.len() > 500 {
            self.history.remove(0);
        }
        self.current = next;
        Ok(())
    }
    pub fn back(&mut self) -> bool {
        if let Some(previous) = self.history.pop() {
            self.current = previous;
            true
        } else {
            false
        }
    }
    pub fn save(&self, p: &Project) -> SaveGame {
        SaveGame {
            version: 1,
            project_id: p.id.clone(),
            fingerprint: fingerprint(p),
            runtime: self.clone(),
            saved_at: crate::project_io::stamp(),
            node_title: p
                .node(&self.current.node)
                .map(|n| n.title.clone())
                .unwrap_or_default(),
        }
    }
    pub fn load(p: &Project, s: SaveGame) -> Result<Self> {
        ensure!(
            s.version == 1 && s.project_id == p.id && s.fingerprint == fingerprint(p),
            "此存档属于不同项目或旧版剧情"
        );
        ensure!(s.runtime.history.len() <= 500, "存档回退历史超出上限");
        for step in std::iter::once(&s.runtime.current).chain(s.runtime.history.iter()) {
            ensure!(p.node(&step.node).is_some(), "存档包含不存在的节点");
            ensure!(
                step.flags.len() == p.flags.len() && step.flags.keys().eq(p.flags.keys()),
                "存档变量与项目不匹配"
            );
        }
        Ok(s.runtime)
    }
}
