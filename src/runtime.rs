use crate::model::{NodeKind, Project};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub node: String,
    pub flags: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, crate::logic::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<crate::production::StageState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_start: Option<crate::production::StageState>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Runtime {
    pub current: Step,
    pub history: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    #[serde(default)]
    pub compatibility: Option<crate::compatibility::SaveCompatibility>,
}
pub fn fingerprint(project: &Project) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(project).unwrap_or_default())
    )
}
impl Runtime {
    pub fn new(p: &Project) -> Self {
        Self::at(p, &p.start)
    }
    pub fn at(p: &Project, node: &str) -> Self {
        let mut runtime = Self {
            current: Step {
                node: node.into(),
                flags: p.flags.clone(),
                variables: p.variables.clone(),
                stage: None,
                scene_start: None,
            },
            history: vec![],
            error: None,
        };
        match Self::enter(p, runtime.current.clone(), node) {
            Ok(step) => runtime.current = step,
            Err(e) => runtime.error = Some(e.to_string()),
        }
        runtime
    }
    fn enter(p: &Project, mut step: Step, id: &str) -> Result<Step> {
        let node = p
            .node(id)
            .ok_or_else(|| anyhow::anyhow!("目标节点 {id} 不存在"))?;
        step.node = id.into();
        for effect in &node.direction.enter {
            effect.apply(p, &mut step)?;
        }
        if p.uses_production() {
            let start = crate::production::StageState::entry(p, node, step.stage.as_ref());
            step.stage = Some(start.finished(node));
            step.scene_start = Some(start);
        }
        Ok(step)
    }
    pub fn jump(&mut self, p: &Project, id: &str) -> Result<()> {
        ensure!(self.error.is_none(), "初始节点执行失败");
        let next = Self::enter(p, self.current.clone(), id)?;
        self.commit(next);
        Ok(())
    }
    fn commit(&mut self, next: Step) {
        self.history.push(self.current.clone());
        if self.history.len() > 500 {
            self.history.remove(0);
        }
        self.current = next;
    }
    pub fn visible_choices(&self, p: &Project) -> Vec<usize> {
        p.node(&self.current.node)
            .filter(|n| n.kind == NodeKind::Choice)
            .map(|n| {
                n.choices
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        crate::logic::choice_available(c, &self.current).unwrap_or(false)
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn advance(&mut self, p: &Project, choice: Option<usize>) -> Result<()> {
        ensure!(
            self.error.is_none(),
            "初始节点执行失败：{}",
            self.error.as_deref().unwrap_or_default()
        );
        let node = p
            .node(&self.current.node)
            .ok_or_else(|| anyhow::anyhow!("当前节点不存在"))?;
        let mut next = self.current.clone();
        // Conditions are checked on the displayed state; exit/choice/entry effects commit together.
        if node.kind == NodeKind::Choice {
            let index = choice.ok_or_else(|| anyhow::anyhow!("请先选择选项"))?;
            ensure!(self.visible_choices(p).contains(&index), "此选项不可用");
        }
        for effect in &node.direction.exit {
            effect.apply(p, &mut next)?;
        }
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
                for effect in &c.effects {
                    effect.apply(p, &mut next)?;
                }
            }
        }
        let target = next.node.clone();
        let next = Self::enter(p, next, &target)?;
        self.commit(next);
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
        self.save_with_fingerprint(p, &fingerprint(p))
    }
    pub fn save_with_fingerprint(&self, p: &Project, content_fingerprint: &str) -> SaveGame {
        SaveGame {
            version: 4,
            project_id: p.id.clone(),
            fingerprint: content_fingerprint.to_owned(),
            runtime: self.clone(),
            saved_at: crate::project_io::stamp(),
            node_title: p
                .node(&self.current.node)
                .map(|n| n.title.clone())
                .unwrap_or_default(),
            compatibility: Some(crate::compatibility::capture(p, self)),
        }
    }
    pub fn load(p: &Project, s: SaveGame) -> Result<Self> {
        crate::compatibility::load(p, s).map(|(runtime, _)| runtime)
    }
}
