use crate::{
    logic::{self, Value},
    model::Project,
    runtime::{Runtime, SaveGame},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    Advance { choice: Option<usize> },
    Back,
    Set { variable: String, value: Value },
    Jump { node: String },
}
impl Command {
    pub fn apply(&self, p: &Project, r: &mut Runtime) -> Result<()> {
        match self {
            Self::Advance { choice } => r.advance(p, *choice),
            Self::Back => {
                ensure!(r.back(), "已在路线开头");
                Ok(())
            }
            Self::Set { variable, value } => logic::set(p, &mut r.current, variable, value.clone()),
            Self::Jump { node } => {
                ensure!(p.node(node).is_some(), "调试目标节点不存在");
                r.history.push(r.current.clone());
                if r.history.len() > 500 {
                    r.history.remove(0);
                }
                r.current.node = node.clone();
                Ok(())
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub version: u32,
    pub project_id: String,
    pub fingerprint: String,
    pub initial: SaveGame,
    pub commands: Vec<Command>,
    pub expected: Runtime,
}
impl Route {
    pub fn new(p: &Project, r: &Runtime) -> Self {
        Self {
            version: 1,
            project_id: p.id.clone(),
            fingerprint: crate::runtime::fingerprint(p),
            initial: r.save(p),
            commands: vec![],
            expected: r.clone(),
        }
    }
    pub fn record(&mut self, command: Command, r: &Runtime) -> Result<()> {
        ensure!(
            self.commands.len() < 10000,
            "路线最多记录 10000 个操作，请重新开始记录"
        );
        self.commands.push(command);
        self.expected = r.clone();
        Ok(())
    }
    pub fn replay(&self, p: &Project) -> Result<Runtime> {
        self.replay_with_coverage(p).map(|(runtime, _)| runtime)
    }
    pub fn replay_with_coverage(&self, p: &Project) -> Result<(Runtime, BTreeSet<String>)> {
        ensure!(
            self.version == 1 && self.project_id == p.id,
            "路线格式或作品标识不匹配"
        );
        ensure!(
            self.fingerprint == crate::runtime::fingerprint(p),
            "项目内容已变化，请从当前版本重新录制路线"
        );
        ensure!(self.commands.len() <= 10000, "路线操作数超出上限");
        let mut runtime = Runtime::load(p, self.initial.clone())?;
        let mut visited = BTreeSet::from([runtime.current.node.clone()]);
        for (i, command) in self.commands.iter().enumerate() {
            command
                .apply(p, &mut runtime)
                .map_err(|e| anyhow::anyhow!("第 {} 个操作复现失败：{e}", i + 1))?;
            visited.insert(runtime.current.node.clone());
        }
        ensure!(
            runtime == self.expected,
            "复现结果与录制时的节点、变量或历史不一致"
        );
        Ok((runtime, visited))
    }
}
#[derive(Default)]
pub struct Debugger {
    pub open: bool,
    pub breakpoints: BTreeSet<String>,
    pub running: bool,
    pub tainted: bool,
    pub route: Option<Route>,
    pub jump: String,
    pub message: String,
    pub visited: BTreeSet<String>,
    pub endings: BTreeSet<String>,
}
impl Debugger {
    pub fn start(&mut self, p: &Project, r: &Runtime) {
        self.route = Some(Route::new(p, r));
        self.running = false;
        self.tainted = false;
        self.message = "已开始记录本次试玩路线".into();
        self.visited.clear();
        self.endings.clear();
        self.observe(p, r);
    }
    pub fn observe(&mut self, p: &Project, r: &Runtime) {
        self.visited.insert(r.current.node.clone());
        if p.node(&r.current.node)
            .is_some_and(|n| n.kind == crate::model::NodeKind::Ending)
        {
            self.endings.insert(r.current.node.clone());
        }
        if self.breakpoints.contains(&r.current.node) {
            self.running = false;
            self.message = format!("断点停下：{}", r.current.node);
        }
    }
}
