//! Bounded branch exploration and author-owned regression cases.
use crate::{
    debugger::{Command, Route},
    logic::{self, Value},
    model::*,
    runtime::*,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TestCase {
    #[serde(default = "default_from_start")]
    pub from_start: bool,
    pub name: String,
    pub initial: Runtime,
    pub commands: Vec<Command>,
    pub expected_node: String,
    pub expected_values: BTreeMap<String, Value>,
}
fn default_from_start() -> bool {
    true
}
impl TestCase {
    pub fn from_route(name: String, route: &Route, p: &Project) -> Self {
        Self {
            from_start: route.initial.runtime.history.is_empty()
                && route.initial.runtime.current.node == p.start
                && values(&route.initial.runtime.current) == values(&Runtime::new(p).current),
            name,
            initial: route.initial.runtime.clone(),
            commands: route.commands.clone(),
            expected_node: route.expected.current.node.clone(),
            expected_values: values(&route.expected.current),
        }
    }
    pub fn run(&self, p: &Project) -> Result<Runtime> {
        ensure!(
            self.commands.len() <= 10000 && self.initial.history.len() <= 500,
            "用例操作或初始历史超过上限"
        );
        ensure!(self.initial.error.is_none(), "用例初始状态无效");
        for step in std::iter::once(&self.initial.current).chain(&self.initial.history) {
            ensure!(
                p.node(&step.node).is_some(),
                "用例初始节点不存在：{}",
                step.node
            );
            let mut check = Runtime::at(p, &step.node);
            let saved = values(step);
            ensure!(
                saved.len() == logic::definitions(p).len(),
                "用例变量声明已变化，请重新录制"
            );
            for (key, value) in saved {
                logic::set(p, &mut check.current, &key, value)?;
            }
        }
        let mut runtime = if self.from_start {
            Runtime::new(p)
        } else {
            self.initial.clone()
        };
        ensure!(runtime.error.is_none(), "当前开场执行失败");
        for (i, command) in self.commands.iter().enumerate() {
            command
                .apply(p, &mut runtime)
                .map_err(|e| anyhow::anyhow!("第 {} 个操作：{e}", i + 1))?;
        }
        ensure!(
            runtime.current.node == self.expected_node,
            "预期节点 {}，实际 {}",
            self.expected_node,
            runtime.current.node
        );
        ensure!(
            values(&runtime.current) == self.expected_values,
            "最终变量与预期不一致"
        );
        Ok(runtime)
    }
}
pub fn values(step: &Step) -> BTreeMap<String, Value> {
    step.flags
        .iter()
        .map(|(k, v)| (k.clone(), Value::Bool(*v)))
        .chain(step.variables.clone())
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TestReport {
    pub project_id: String,
    pub fingerprint: String,
    pub states: usize,
    pub limit: usize,
    pub truncated: bool,
    pub nodes: BTreeSet<String>,
    pub choices: BTreeSet<(String, usize)>,
    pub endings: BTreeSet<String>,
    pub uncovered_nodes: Vec<String>,
    pub uncovered_choices: Vec<(String, usize)>,
    pub uncovered_endings: Vec<String>,
    pub errors: Vec<String>,
    pub cases: Vec<CaseResult>,
    pub routes: Vec<TestCase>,
}
impl TestReport {
    pub fn passed(&self) -> bool {
        self.errors.is_empty() && self.cases.iter().all(|c| c.passed)
    }
}
pub fn run(p: &Project, limit: usize) -> TestReport {
    let limit = limit.clamp(1, 8192);
    let mut report = TestReport {
        project_id: p.id.clone(),
        fingerprint: fingerprint(p),
        limit,
        ..Default::default()
    };
    let initial = Runtime::new(p);
    if let Some(error) = &initial.error {
        report.errors.push(error.clone());
        return report;
    }
    let mut seen = HashSet::new();
    let mut pending = VecDeque::from([(initial.clone(), Vec::<Command>::new())]);
    // Stop both state count and command-path depth; numeric loops cannot run forever.
    while let Some((runtime, commands)) = pending.pop_front() {
        let key = (
            runtime.current.node.clone(),
            runtime.current.flags.clone(),
            runtime.current.variables.clone(),
        );
        if seen.contains(&key) {
            continue;
        }
        if seen.len() >= limit {
            report.truncated = true;
            break;
        }
        seen.insert(key);
        report.nodes.insert(runtime.current.node.clone());
        let Some(node) = p.node(&runtime.current.node) else {
            report.errors.push("运行到缺失节点".into());
            continue;
        };
        if node.kind == NodeKind::Ending {
            if report.endings.insert(node.id.clone()) {
                report.routes.push(TestCase {
                    from_start: true,
                    name: format!("自动路线 · {}", node.title),
                    initial: initial.clone(),
                    commands,
                    expected_node: node.id.clone(),
                    expected_values: values(&runtime.current),
                });
            }
            continue;
        }
        if commands.len() >= 1000 {
            report.truncated = true;
            continue;
        }
        let options = if node.kind == NodeKind::Choice {
            runtime
                .visible_choices(p)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>()
        } else {
            vec![None]
        };
        if options.is_empty() {
            report.errors.push(format!("{}：没有可见选项", node.id));
        }
        for choice in options {
            let mut next = runtime.clone();
            next.history.clear();
            let command = Command::Advance { choice };
            match command.apply(p, &mut next) {
                Ok(()) => {
                    if let Some(index) = choice {
                        report.choices.insert((node.id.clone(), index));
                    }
                    let mut path = commands.clone();
                    path.push(command);
                    pending.push_back((next, path));
                }
                Err(e) => report.errors.push(format!("{}：{e}", node.id)),
            }
        }
    }
    report.states = seen.len();
    for node in &p.nodes {
        if !report.nodes.contains(&node.id) {
            report.uncovered_nodes.push(node.id.clone());
        }
        if node.kind == NodeKind::Ending && !report.endings.contains(&node.id) {
            report.uncovered_endings.push(node.id.clone());
        }
        for i in 0..node.choices.len() {
            if !report.choices.contains(&(node.id.clone(), i)) {
                report.uncovered_choices.push((node.id.clone(), i));
            }
        }
    }
    report.cases = p
        .production
        .cases
        .iter()
        .map(|case| match case.run(p) {
            Ok(_) => CaseResult {
                name: case.name.clone(),
                passed: true,
                message: "节点与变量符合预期".into(),
            },
            Err(e) => CaseResult {
                name: case.name.clone(),
                passed: false,
                message: e.to_string(),
            },
        })
        .collect();
    report
}
