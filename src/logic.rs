//! Typed story values and bounded, data-only conditions. No script execution.
use crate::{model::*, runtime::Step};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(i64),
    Text(String),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Bool,
    Number,
    Text,
}
impl ValueKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bool => "布尔",
            Self::Number => "整数",
            Self::Text => "文本",
        }
    }
    pub fn initial(self) -> Value {
        match self {
            Self::Bool => Value::Bool(false),
            Self::Number => Value::Number(0),
            Self::Text => Value::Text(String::new()),
        }
    }
}
impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Self::Bool(_) => ValueKind::Bool,
            Self::Number(_) => ValueKind::Number,
            Self::Text(_) => ValueKind::Text,
        }
    }
    pub fn display(&self) -> String {
        match self {
            Self::Bool(v) => if *v { "真" } else { "假" }.into(),
            Self::Number(v) => v.to_string(),
            Self::Text(v) => v.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}
impl Compare {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eq => "等于",
            Self::Ne => "不等于",
            Self::Gt => "大于",
            Self::Ge => "大于等于",
            Self::Lt => "小于",
            Self::Le => "小于等于",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    Compare {
        variable: String,
        op: Compare,
        value: Value,
    },
    All {
        items: Vec<Condition>,
    },
    Any {
        items: Vec<Condition>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    Set { variable: String, value: Value },
    Add { variable: String, amount: i64 },
}
pub fn definitions(p: &Project) -> BTreeMap<String, Value> {
    p.flags
        .iter()
        .map(|(k, v)| (k.clone(), Value::Bool(*v)))
        .chain(p.variables.iter().map(|(k, v)| (k.clone(), v.clone())))
        .collect()
}
pub fn get(step: &Step, key: &str) -> Option<Value> {
    step.variables
        .get(key)
        .cloned()
        .or_else(|| step.flags.get(key).map(|v| Value::Bool(*v)))
}
pub fn set(p: &Project, step: &mut Step, key: &str, value: Value) -> Result<()> {
    let initial = p
        .variables
        .get(key)
        .cloned()
        .or_else(|| p.flags.get(key).map(|v| Value::Bool(*v)))
        .ok_or_else(|| anyhow::anyhow!("变量 {key} 未定义"))?;
    ensure!(
        initial.kind() == value.kind(),
        "变量 {key} 类型不匹配，需要{}",
        initial.kind().label()
    );
    if let Value::Text(text) = &value {
        ensure!(text.len() <= 65536, "文本变量最多 64 KiB");
    }
    if p.flags.contains_key(key) {
        if let Value::Bool(v) = value {
            step.flags.insert(key.into(), v);
        }
    } else {
        step.variables.insert(key.into(), value);
    }
    Ok(())
}
impl Condition {
    pub fn validate(&self, p: &Project) -> Result<()> {
        let mut count = 0;
        self.check(&definitions(p), 0, &mut count)
    }
    fn check(&self, defs: &BTreeMap<String, Value>, depth: usize, count: &mut usize) -> Result<()> {
        *count += 1;
        ensure!(depth <= 8 && *count <= 128, "条件最多嵌套 8 层、128 项");
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => {
                let initial = defs
                    .get(variable)
                    .ok_or_else(|| anyhow::anyhow!("变量 {variable} 未定义"))?;
                ensure!(
                    initial.kind() == value.kind(),
                    "条件变量 {variable} 类型不匹配"
                );
                ensure!(
                    matches!(op, Compare::Eq | Compare::Ne) || value.kind() == ValueKind::Number,
                    "大小比较只支持整数变量"
                );
            }
            Self::All { items } | Self::Any { items } => {
                ensure!(!items.is_empty(), "条件组不能为空");
                for item in items {
                    item.check(defs, depth + 1, count)?;
                }
            }
        }
        Ok(())
    }
    pub fn evaluate(&self, step: &Step) -> Result<bool> {
        self.eval(step, 0, &mut 0)
    }
    fn eval(&self, step: &Step, depth: usize, count: &mut usize) -> Result<bool> {
        *count += 1;
        ensure!(depth <= 8 && *count <= 128, "条件超出计算上限");
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => {
                let actual =
                    get(step, variable).ok_or_else(|| anyhow::anyhow!("变量 {variable} 不存在"))?;
                ensure!(actual.kind() == value.kind(), "变量 {variable} 类型不匹配");
                Ok(match op {
                    Compare::Eq => actual == *value,
                    Compare::Ne => actual != *value,
                    _ => {
                        let (Value::Number(a), Value::Number(b)) = (&actual, value) else {
                            bail!("大小比较只支持整数")
                        };
                        match op {
                            Compare::Gt => a > b,
                            Compare::Ge => a >= b,
                            Compare::Lt => a < b,
                            Compare::Le => a <= b,
                            _ => unreachable!(),
                        }
                    }
                })
            }
            Self::All { items } => {
                ensure!(!items.is_empty(), "条件组不能为空");
                let mut yes = true;
                for item in items {
                    yes &= item.eval(step, depth + 1, count)?;
                }
                Ok(yes)
            }
            Self::Any { items } => {
                ensure!(!items.is_empty(), "条件组不能为空");
                let mut yes = false;
                for item in items {
                    yes |= item.eval(step, depth + 1, count)?;
                }
                Ok(yes)
            }
        }
    }
    pub fn describe(&self, step: &Step) -> String {
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => format!(
                "{variable} 当前{}，要求{} {}",
                get(step, variable)
                    .map(|v| v.display())
                    .unwrap_or_else(|| "未定义".into()),
                op.label(),
                value.display()
            ),
            Self::All { items } => format!(
                "全部满足：[{}]",
                items
                    .iter()
                    .map(|c| c.describe(step))
                    .collect::<Vec<_>>()
                    .join("；")
            ),
            Self::Any { items } => format!(
                "任一满足：[{}]",
                items
                    .iter()
                    .map(|c| c.describe(step))
                    .collect::<Vec<_>>()
                    .join("；")
            ),
        }
    }
}
impl Effect {
    pub fn validate(&self, p: &Project) -> Result<()> {
        let defs = definitions(p);
        match self {
            Self::Set { variable, value } => {
                ensure!(
                    defs.get(variable).is_some_and(|v| v.kind() == value.kind()),
                    "赋值变量 {variable} 未定义或类型不匹配"
                );
                if let Value::Text(v) = value {
                    ensure!(v.len() <= 65536, "文本变量最多 64 KiB");
                }
            }
            Self::Add { variable, .. } => ensure!(
                defs.get(variable)
                    .is_some_and(|v| matches!(v, Value::Number(_))),
                "加减变量 {variable} 必须为整数"
            ),
        }
        Ok(())
    }
    pub fn apply(&self, p: &Project, step: &mut Step) -> Result<()> {
        self.validate(p)?;
        match self {
            Self::Set { variable, value } => set(p, step, variable, value.clone()),
            Self::Add { variable, amount } => {
                let Some(Value::Number(old)) = get(step, variable) else {
                    bail!("整数变量不存在")
                };
                let value = old
                    .checked_add(*amount)
                    .ok_or_else(|| anyhow::anyhow!("变量 {variable} 加减溢出"))?;
                set(p, step, variable, Value::Number(value))
            }
        }
    }
}
pub fn choice_available(choice: &Choice, step: &Step) -> Result<bool> {
    Ok(choice
        .condition
        .as_ref()
        .is_none_or(|f| step.flags.get(&f.flag) == Some(&f.value))
        && match &choice.when {
            Some(c) => c.evaluate(step)?,
            None => true,
        })
}
pub fn choice_reason(choice: &Choice, step: &Step) -> String {
    let mut parts = vec![];
    if let Some(f) = &choice.condition {
        parts.push(format!(
            "{} 当前{}，要求{}",
            f.flag,
            step.flags
                .get(&f.flag)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "未定义".into()),
            f.value
        ));
    }
    if let Some(c) = &choice.when {
        parts.push(c.describe(step));
    }
    if parts.is_empty() {
        "无条件，可见".into()
    } else {
        parts.join("；")
    }
}
