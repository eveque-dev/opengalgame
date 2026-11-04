use crate::{model::*, project_io::*, recovery};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStage {
    #[default]
    Draft,
    Production,
    Complete,
    Archived,
}
impl ProjectStage {
    pub fn label(&self) -> &str {
        match self {
            Self::Draft => "构思中",
            Self::Production => "创作中",
            Self::Complete => "已完成",
            Self::Archived => "已归档",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectCard {
    pub path: PathBuf,
    pub title: String,
    pub pinned: bool,
    pub stage: ProjectStage,
    pub last_opened: u128,
    pub nodes: usize,
    pub words: usize,
    pub tasks_done: usize,
    pub tasks_total: usize,
}
impl ProjectCard {
    pub fn refresh(&mut self, p: &Project) {
        self.title = p.title.clone();
        self.nodes = p.nodes.len();
        self.words = p
            .nodes
            .iter()
            .map(|n| n.text.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        self.tasks_total = p.tasks.len();
        self.tasks_done = p
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceCatalog {
    pub projects: Vec<ProjectCard>,
    pub open_paths: Vec<PathBuf>,
    pub active_path: Option<PathBuf>,
}
pub fn path_key(path: &Path) -> String {
    recovery::normalized(path)
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_lowercase()
}
impl WorkspaceCatalog {
    pub fn load() -> Self {
        let mut catalog: Self = std::fs::read(recovery::state_dir().join("workspace.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for path in recovery::recent().files {
            if !catalog
                .projects
                .iter()
                .any(|p| path_key(&p.path) == path_key(&path))
            {
                catalog.projects.push(ProjectCard {
                    title: path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "未命名项目".into()),
                    path,
                    ..Default::default()
                });
            }
        }
        catalog
    }
    pub fn save(&self) -> Result<()> {
        write_json(&recovery::state_dir().join("workspace.json"), self)
    }
    pub fn register(&mut self, path: &Path, p: &Project) -> usize {
        let key = path_key(path);
        let index = self
            .projects
            .iter()
            .position(|c| path_key(&c.path) == key)
            .unwrap_or_else(|| {
                self.projects.push(ProjectCard {
                    path: recovery::normalized(path),
                    ..Default::default()
                });
                self.projects.len() - 1
            });
        self.projects[index].refresh(p);
        self.projects[index].last_opened = stamp();
        index
    }
}
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub fn unique_id() -> String {
    format!(
        "opengalgame-{}-{}-{}",
        stamp(),
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTemplate {
    Blank,
    Branching,
}
pub fn template_project(title: &str, template: ProjectTemplate) -> Project {
    let mut opening = Node::new("opening".into(), NodeKind::Dialogue, 0);
    opening.title = "故事的开始".into();
    opening.text = "你的故事，从第一句对白开始。".into();
    opening.next = Some(
        if template == ProjectTemplate::Blank {
            "ending"
        } else {
            "choice"
        }
        .into(),
    );
    let mut ending = Node::new("ending".into(), NodeKind::Ending, 1);
    ending.title = "故事终章".into();
    ending.text = "为这个结局写下你的答案。".into();
    let mut nodes = vec![opening, ending];
    if template == ProjectTemplate::Branching {
        let mut choice = Node::new("choice".into(), NodeKind::Choice, 1);
        choice.title = "第一次选择".into();
        choice.text = "你会选择哪条路？".into();
        choice.choices = vec![
            Choice {
                text: "留下".into(),
                target: "ending".into(),
                condition: None,
                set_flag: None,
            },
            Choice {
                text: "出发".into(),
                target: "another_ending".into(),
                condition: None,
                set_flag: None,
            },
        ];
        let mut other = Node::new("another_ending".into(), NodeKind::Ending, 3);
        other.title = "另一种明天".into();
        other.text = "另一个结局，同样值得被写下来。".into();
        nodes.insert(1, choice);
        nodes.push(other);
    }
    let mut p = Project {
        format_version: 1,
        id: unique_id(),
        title: title.trim().to_owned(),
        author: String::new(),
        start: "opening".into(),
        characters: vec![],
        backgrounds: vec![],
        flags: Default::default(),
        nodes: nodes.into(),
        audio: vec![],
        presentation: Default::default(),
        tasks: vec![],
        save_compatibility: Default::default(),
    };
    crate::model::arrange_nodes(&mut p);
    p
}
pub fn create_project(parent: &Path, title: &str, template: ProjectTemplate) -> Result<PathBuf> {
    ensure!(!title.trim().is_empty(), "请先填写作品名称");
    ensure!(parent.is_dir(), "请选择有效的项目父目录");
    let p = template_project(title, template);
    let directory = parent.join(&p.id);
    std::fs::create_dir(&directory)?;
    let path = directory.join("project.json");
    save_project(&path, &p)?;
    Ok(path)
}
pub fn fork_project(p: &Project, base: &Path, parent: &Path, title: &str) -> Result<PathBuf> {
    ensure!(!title.trim().is_empty(), "请填写副本名称");
    ensure!(parent.is_dir(), "请选择有效的父目录");
    let mut copy = p.clone();
    copy.id = unique_id();
    copy.title = title.trim().to_owned();
    copy.save_compatibility = Default::default();
    let directory = parent.join(&copy.id);
    std::fs::create_dir(&directory)?;
    let path = directory.join("project.json");
    clone_project(&copy, base, &path)?;
    Ok(path)
}
