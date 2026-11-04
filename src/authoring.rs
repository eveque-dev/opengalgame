use crate::model::*;
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

pub fn chapters(project: &Project) -> Vec<String> {
    let mut seen = BTreeSet::new();
    project
        .nodes
        .iter()
        .map(|n| {
            if n.chapter.is_empty() {
                "未分章".to_owned()
            } else {
                n.chapter.clone()
            }
        })
        .filter(|c| seen.insert(c.clone()))
        .collect()
}
pub fn matches(node: &Node, query: &str) -> bool {
    query.is_empty()
        || node.title.contains(query)
        || node.id.contains(query)
        || node.chapter.contains(query)
        || node.text.contains(query)
        || node.choices.iter().any(|c| c.text.contains(query))
}
pub fn replace_all(project: &mut Project, query: &str, replacement: &str) -> usize {
    if query.is_empty() {
        return 0;
    }
    let mut count = 0;
    for node in &mut project.nodes {
        for text in [&mut node.title, &mut node.text]
            .into_iter()
            .chain(node.choices.iter_mut().map(|c| &mut c.text))
        {
            count += text.matches(query).count();
            *text = text.replace(query, replacement);
        }
    }
    count
}
pub fn import_dialogue(
    project: &mut Project,
    source: &str,
    after: usize,
    default_chapter: &str,
) -> Result<usize> {
    ensure!(source.len() <= 1024 * 1024, "一次最多导入 1 MB 文本");
    let mut draft = project.clone();
    let anchor = draft.nodes.get(after).cloned();
    ensure!(
        anchor.as_ref().is_some_and(|n| n.kind != NodeKind::Choice),
        "请选择对白或结局节点插入；选择节点请通过端口连接新分支"
    );
    let insert = (after + 1).min(draft.nodes.len());
    let terminal = anchor
        .as_ref()
        .and_then(|n| {
            if n.kind == NodeKind::Ending {
                Some(n.id.clone())
            } else {
                n.next.clone()
            }
        })
        .or_else(|| {
            draft
                .nodes
                .iter()
                .find(|n| n.kind == NodeKind::Ending)
                .map(|n| n.id.clone())
        });
    ensure!(terminal.is_some(), "请先添加一个结局节点，再导入连续对白");
    let mut chapter = default_chapter.to_owned();
    let mut created = vec![];
    let mut reserved: std::collections::HashSet<String> =
        draft.nodes.iter().map(|n| n.id.clone()).collect();
    let mut next_import = 1usize;
    let mut background = anchor.as_ref().and_then(|n| n.background.clone());
    for line in source.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if line.starts_with('[') && line.ends_with(']') {
            chapter = line[1..line.len() - 1].trim().to_owned();
            continue;
        }
        if let Some(name) = line
            .strip_prefix("@背景：")
            .or_else(|| line.strip_prefix("@背景:"))
        {
            background = Some(
                draft
                    .backgrounds
                    .iter()
                    .find(|b| b.id == name.trim() || b.name == name.trim())
                    .ok_or_else(|| anyhow::anyhow!("找不到背景：{}", name.trim()))?
                    .id
                    .clone(),
            );
            continue;
        }
        let pair = line.split_once('：').or_else(|| line.split_once(':'));
        let (speaker, text) = match pair {
            Some((s, t)) if s.chars().count() <= 24 => (Some(s.trim()), t.trim()),
            _ => (None, line),
        };
        ensure!(!text.is_empty(), "对白正文不能为空");
        let character = if let Some(name) = speaker.filter(|s| !matches!(*s, "旁白" | "尼克·手记"))
        {
            if let Some(c) = draft
                .characters
                .iter()
                .find(|c| c.name == name || c.id == name)
            {
                Some(c.id.clone())
            } else {
                let id = (1..)
                    .map(|n| format!("cast_{n}"))
                    .find(|id| draft.characters.iter().all(|c| &c.id != id))
                    .unwrap();
                draft.characters.push(Character {
                    id: id.clone(),
                    name: name.to_owned(),
                    color: [190, 185, 240],
                    portrait: None,
                    expressions: Default::default(),
                    tags: String::new(),
                });
                Some(id)
            }
        } else {
            None
        };
        let id = loop {
            let id = format!("import_{next_import}");
            next_import += 1;
            if reserved.insert(id.clone()) {
                break id;
            }
        };
        let mut node = Node::new(id, NodeKind::Dialogue, insert + created.len());
        node.title = text.chars().take(14).collect();
        node.text = text.to_owned();
        node.character = character;
        node.background = background.clone();
        node.chapter = chapter.clone();
        created.push(node);
    }
    ensure!(!created.is_empty(), "没有找到可导入的对白");
    let count = created.len();
    let first = created[0].id.clone();
    for i in 0..created.len() {
        created[i].next = if i + 1 < created.len() {
            Some(created[i + 1].id.clone())
        } else {
            terminal.clone()
        };
    }
    if let Some(anchor) = anchor {
        if anchor.kind == NodeKind::Dialogue {
            draft.nodes[after].next = Some(first.clone());
        } else if anchor.kind == NodeKind::Ending {
            if draft.start == anchor.id {
                draft.start = first.clone();
            }
            for node in &mut draft.nodes {
                if node.next.as_deref() == Some(&anchor.id) {
                    node.next = Some(first.clone());
                }
                for choice in &mut node.choices {
                    if choice.target == anchor.id {
                        choice.target = first.clone();
                    }
                }
            }
        }
    }
    draft.nodes.splice(insert..insert, created);
    *project = draft;
    Ok(count)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub node: String,
    pub choice: Option<usize>,
}
pub fn connect(project: &mut Project, port: &Port, target: &str) -> Result<()> {
    ensure!(project.node(target).is_some(), "目标节点不存在");
    let node = project
        .nodes
        .iter_mut()
        .find(|n| n.id == port.node)
        .ok_or_else(|| anyhow::anyhow!("起点不存在"))?;
    match (node.kind, port.choice) {
        (NodeKind::Dialogue, None) => node.next = Some(target.to_owned()),
        (NodeKind::Choice, Some(i)) => {
            node.choices
                .get_mut(i)
                .ok_or_else(|| anyhow::anyhow!("选项不存在"))?
                .target = target.to_owned()
        }
        _ => anyhow::bail!("此端口不能连接"),
    };
    Ok(())
}
pub fn cycle_nodes(project: &Project) -> BTreeSet<String> {
    let ids: std::collections::HashMap<_, _> = project
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let graph: Vec<Vec<usize>> = project
        .nodes
        .iter()
        .map(|n| {
            n.targets()
                .into_iter()
                .filter_map(|t| ids.get(t).copied())
                .collect()
        })
        .collect();
    let mut reverse = vec![vec![]; graph.len()];
    for (i, edges) in graph.iter().enumerate() {
        for &j in edges {
            reverse[j].push(i);
        }
    }
    let mut seen = vec![false; graph.len()];
    let mut order = Vec::with_capacity(graph.len());
    for root in 0..graph.len() {
        let mut stack = vec![(root, false)];
        while let Some((i, done)) = stack.pop() {
            if done {
                order.push(i);
            } else if !seen[i] {
                seen[i] = true;
                stack.push((i, true));
                for &j in graph[i].iter().rev() {
                    if !seen[j] {
                        stack.push((j, false));
                    }
                }
            }
        }
    }
    seen.fill(false);
    let mut cycles = BTreeSet::new();
    for root in order.into_iter().rev() {
        if seen[root] {
            continue;
        }
        let mut stack = vec![root];
        seen[root] = true;
        let mut component = vec![];
        while let Some(i) = stack.pop() {
            component.push(i);
            for &j in &reverse[i] {
                if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if component.len() > 1 || graph[root].contains(&root) {
            cycles.extend(component.into_iter().map(|i| project.nodes[i].id.clone()));
        }
    }
    cycles
}
