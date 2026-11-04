//! RFC 4180-style UTF-8 CSV, transactional import with stable identities.
use crate::{model::*, project_io::read_project};
use anyhow::{Result, ensure};
use std::path::Path;
pub const HEADER: [&str; 9] = [
    "节点标识",
    "章节",
    "类型",
    "标题",
    "发言人",
    "正文",
    "背景",
    "下一节点",
    "演出与选项",
];
pub fn parse(text: &str) -> Result<Vec<Vec<String>>> {
    ensure!(text.len() <= 20 * 1024 * 1024, "CSV 最多 20 MB");
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                ensure!(field.is_empty() && !closed, "CSV 引号位置无效");
                quoted = true;
            }
            ',' => {
                row.push(std::mem::take(&mut field));
                closed = false;
            }
            '\n' | '\r' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                closed = false;
                ensure!(rows.len() <= 10001, "CSV 最多 10000 个节点");
            }
            _ => {
                ensure!(!closed, "CSV 引号结束后必须是逗号或换行");
                field.push(c);
            }
        }
    }
    ensure!(!quoted, "CSV 引号未闭合");
    if !field.is_empty() || !row.is_empty() || closed {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}
pub fn export(p: &Project) -> Result<String> {
    let mut out = HEADER.map(quote).join(",");
    out.push_str("\r\n");
    for node in &p.nodes {
        let fields = [
            node.id.clone(),
            node.chapter.clone(),
            node.kind.label().into(),
            node.title.clone(),
            node.character.clone().unwrap_or_default(),
            node.text.clone(),
            node.background.clone().unwrap_or_default(),
            node.next.clone().unwrap_or_default(),
            serde_json::to_string(node)?,
        ];
        out.push_str(
            &fields
                .iter()
                .map(|s| quote(s))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    Ok(out)
}
pub struct Preview {
    pub source_fingerprint: String,
    pub project: Project,
    pub changed: usize,
    pub added: usize,
}
pub fn preview(p: &Project, text: &str) -> Result<Preview> {
    let rows = parse(text)?;
    ensure!(rows.len() > 1, "CSV 没有节点数据");
    let header = &rows[0];
    let columns: Vec<usize> = HEADER
        .iter()
        .map(|name| {
            header
                .iter()
                .position(|c| c == name)
                .ok_or_else(|| anyhow::anyhow!("缺少列：{name}"))
        })
        .collect::<Result<_>>()?;
    ensure!(
        header
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == header.len(),
        "CSV 列名重复"
    );
    let mut draft = p.clone();
    let mut seen = std::collections::BTreeSet::new();
    let mut changed = 0;
    let mut added = 0;
    for (line, row) in rows.iter().enumerate().skip(1) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        ensure!(row.len() == header.len(), "第 {} 行列数不符", line + 1);
        let get = |i: usize| row[columns[i]].as_str();
        let id = get(0).trim();
        ensure!(
            !id.is_empty() && seen.insert(id.to_owned()),
            "第 {} 行标识为空或重复",
            line + 1
        );
        let existing = p.node(id);
        ensure!(
            existing.is_some() || !p.save_compatibility.node_aliases.contains_key(id),
            "旧标识 {id} 已保留用于存档迁移"
        );
        let mut node = if get(8).trim().is_empty() {
            existing
                .cloned()
                .unwrap_or_else(|| Node::new(id.into(), NodeKind::Dialogue, draft.nodes.len()))
        } else {
            let node: Node = serde_json::from_str(get(8))
                .map_err(|e| anyhow::anyhow!("第 {} 行演出数据错误：{e}", line + 1))?;
            ensure!(node.id == id, "第 {} 行演出数据与标识不一致", line + 1);
            node
        };
        node.chapter = get(1).into();
        node.kind = match get(2) {
            "对白" | "dialogue" => NodeKind::Dialogue,
            "选择" | "choice" => NodeKind::Choice,
            "结局" | "ending" => NodeKind::Ending,
            other => anyhow::bail!("第 {} 行节点类型无效：{other}", line + 1),
        };
        node.title = get(3).into();
        node.text = get(5).into();
        node.next = (!get(7).trim().is_empty()).then(|| get(7).trim().into());
        node.character = if get(4).trim().is_empty() {
            None
        } else {
            Some(
                p.characters
                    .iter()
                    .find(|c| c.id == get(4) || c.name == get(4))
                    .ok_or_else(|| anyhow::anyhow!("第 {} 行人物不存在", line + 1))?
                    .id
                    .clone(),
            )
        };
        node.background = if get(6).trim().is_empty() {
            None
        } else {
            Some(
                p.backgrounds
                    .iter()
                    .find(|b| b.id == get(6) || b.name == get(6))
                    .ok_or_else(|| anyhow::anyhow!("第 {} 行背景不存在", line + 1))?
                    .id
                    .clone(),
            )
        };
        if let Some(index) = draft.nodes.iter().position(|n| n.id == id) {
            if draft.nodes[index] != node {
                draft.nodes[index] = node;
                changed += 1;
            }
        } else {
            draft.nodes.push(node);
            added += 1;
        }
    }
    ensure!(draft.nodes.len() <= 10000, "项目最多 10000 个节点");
    if draft.uses_typed_logic() {
        draft.format_version = 2;
    }
    let errors: Vec<_> = validate(&draft, None)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .take(8)
        .map(|i| i.message)
        .collect();
    ensure!(errors.is_empty(), "导入未应用：{}", errors.join("；"));
    Ok(Preview {
        source_fingerprint: crate::runtime::fingerprint(p),
        project: draft,
        changed,
        added,
    })
}
pub fn read_csv(path: &Path, p: &Project) -> Result<Preview> {
    ensure!(
        std::fs::metadata(path)?.len() <= 20 * 1024 * 1024,
        "CSV 最多 20 MB"
    );
    preview(p, &std::fs::read_to_string(path)?)
}
pub fn check_project(path: &Path) -> Result<()> {
    let p = read_project(path)?;
    preview(&p, &export(&p)?)?;
    Ok(())
}
