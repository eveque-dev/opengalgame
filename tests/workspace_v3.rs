use opengalgame::{authoring, catalog::*, compatibility::*, model::*, project_io::*, runtime::*};
use std::{collections::BTreeSet, path::PathBuf};

#[test]
fn oversized_project_save_preserves_the_last_readable_file_and_backup() {
    let root = scratch();
    let path = create_project(&root, "边界验证", ProjectTemplate::Blank).unwrap();
    let p = read_project(&path).unwrap();
    let mut huge = p.clone();
    huge.nodes[0].text = "x".repeat(21 * 1024 * 1024);
    assert!(save_project(&path, &huge).is_err());
    assert_eq!(read_project(&path).unwrap(), p);
    assert!(!path.with_extension("json.bak").exists());
}

fn demo() -> Project {
    serde_json::from_str(include_str!("../examples/first-light/project.json")).unwrap()
}

#[test]
fn generated_nodes_and_imports_never_reuse_ids_reserved_for_saved_game_migration() {
    let mut p = template_project("迁移标识", ProjectTemplate::Blank);
    p.rename_node("opening", "scene_1").unwrap();
    p.rename_node("scene_1", "opening").unwrap();
    assert_eq!(p.next_id(), "scene_2");
    authoring::import_dialogue(&mut p, "旁白：第一次导入", 0, "章节").unwrap();
    p.rename_node("import_1", "middle").unwrap();
    authoring::import_dialogue(&mut p, "旁白：第二次导入", 0, "章节").unwrap();
    assert!(p.node("import_1").is_none());
    assert!(p.node("import_2").is_some());
    assert!(validate(&p, None).is_empty());
}
fn scratch() -> PathBuf {
    let p = std::env::temp_dir().join(unique_id());
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn save() -> (Project, Runtime, SaveGame) {
    let p = demo();
    let mut r = Runtime::new(&p);
    r.advance(&p, None).unwrap();
    let s = r.save(&p);
    (p, r, s)
}

#[test]
fn released_v02_fixture_reads_before_and_after_cosmetic_update() {
    let mut p = demo();
    let old: SaveGame = serde_json::from_str(include_str!("fixtures/v0.2-save.json")).unwrap();
    assert_eq!(old.version, 1);
    assert_eq!(legacy_fingerprint(&p), old.fingerprint);
    let (r, report) = load(&p, old.clone()).unwrap();
    assert_eq!(r.current.node, "reply");
    assert!(report.legacy);
    let baseline = p.clone();
    register_legacy_baseline(&mut p, &baseline).unwrap();
    p.nodes[0].text = "新版对白".into();
    p.title = "修订版".into();
    assert_eq!(load(&p, old).unwrap().0, r);
}
#[test]
fn opening_old_project_registers_baseline_and_saves_it_permanently() {
    let root = scratch();
    let path = root.join("project.json");
    std::fs::write(
        &path,
        include_bytes!("../examples/first-light/project.json"),
    )
    .unwrap();
    let mut p = read_project(&path).unwrap();
    assert_eq!(p.save_compatibility.legacy_baselines.len(), 1);
    p.nodes[0].text = "调整内容".into();
    save_project(&path, &p).unwrap();
    let reopened = read_project(&path).unwrap();
    assert_eq!(reopened.save_compatibility.legacy_baselines.len(), 1);
    load(
        &reopened,
        serde_json::from_str(include_str!("fixtures/v0.2-save.json")).unwrap(),
    )
    .unwrap();
}
#[test]
fn legacy_without_verified_baseline_is_rejected_after_update() {
    let mut p = demo();
    p.title.push('!');
    let s = serde_json::from_str(include_str!("fixtures/v0.2-save.json")).unwrap();
    assert!(load(&p, s).unwrap_err().to_string().contains("基线"));
    let other = template_project("其他作品", ProjectTemplate::Blank);
    assert!(register_legacy_baseline(&mut p, &other).is_err());
}
#[test]
fn text_media_layout_tasks_and_unvisited_new_chapter_keep_progress() {
    let (mut p, r, s) = save();
    p.title = "调整后的标题".into();
    p.nodes[0].text = "新正文".into();
    p.nodes[1].position = [900., 50.];
    p.nodes[1].background = None;
    p.tasks.push(CreativeTask {
        id: "review".into(),
        title: "审稿".into(),
        notes: String::new(),
        status: TaskStatus::Doing,
    });
    p.nodes
        .push(Node::new("bonus".into(), NodeKind::Ending, 99));
    let (restored, report) = load(&p, s).unwrap();
    assert_eq!(restored, r);
    assert!(report.content_updated);
}
#[test]
fn visited_flow_changes_reject_but_unvisited_changes_do_not() {
    let (p, _, s) = save();
    let mut changed = p.clone();
    changed.nodes[0].next = Some("train".into());
    assert!(load(&changed, s.clone()).is_err());
    changed = p.clone();
    changed.nodes[1].kind = NodeKind::Ending;
    assert!(load(&changed, s.clone()).is_err());
    changed = p;
    let i = changed.nodes.iter().position(|n| n.id == "train").unwrap();
    changed.nodes[i].text = "后来".into();
    load(&changed, s).unwrap();
}
#[test]
fn choice_conditions_and_order_are_part_of_the_save_contract() {
    let mut p = demo();
    let r = Runtime::at(&p, "letter");
    let s = r.save(&p);
    let index = p.nodes.iter().position(|n| n.id == "letter").unwrap();
    p.nodes[index].choices[0].text = "另一种说法".into();
    load(&p, s.clone()).unwrap();
    p.nodes[index].choices.swap(0, 1);
    assert!(load(&p, s.clone()).is_err());
    p.nodes[index].choices.swap(0, 1);
    p.nodes[index].choices[0].condition = Some(FlagValue {
        flag: "kept_letter".into(),
        value: true,
    });
    assert!(load(&p, s).is_err());
}
#[test]
fn rename_migrates_current_history_and_targets_then_can_be_renamed_back() {
    let (mut p, r, s) = save();
    p.rename_node("arrival", "begin_new").unwrap();
    p.rename_node("reply", "reply_new").unwrap();
    let (restored, report) = load(&p, s.clone()).unwrap();
    assert_eq!(restored.current.node, "reply_new");
    assert_eq!(restored.history[0].node, "begin_new");
    assert_eq!(report.migrated_nodes, 2);
    p.rename_node("begin_new", "arrival").unwrap();
    p.rename_node("reply_new", "reply").unwrap();
    assert_eq!(load(&p, s).unwrap().0, r);
}
#[test]
fn chained_aliases_resolve_and_cyclic_missing_or_incompatible_targets_reject() {
    let (mut p, _, s) = save();
    p.rename_node("reply", "reply2").unwrap();
    p.rename_node("reply2", "reply3").unwrap();
    assert_eq!(load(&p, s.clone()).unwrap().0.current.node, "reply3");
    p.save_compatibility
        .node_aliases
        .insert("reply".into(), "loop".into());
    p.save_compatibility
        .node_aliases
        .insert("loop".into(), "reply".into());
    assert!(load(&p, s.clone()).is_err());
    p.save_compatibility
        .node_aliases
        .insert("reply".into(), "missing".into());
    assert!(load(&p, s).is_err());
}
#[test]
fn added_flags_use_defaults_for_every_history_step_removed_flags_reject() {
    let (mut p, r, s) = save();
    p.flags.insert("new_flag".into(), true);
    let (restored, report) = load(&p, s.clone()).unwrap();
    for step in std::iter::once(&restored.current).chain(&restored.history) {
        assert!(step.flags["new_flag"]);
        assert_eq!(step.flags["kept_letter"], r.current.flags["kept_letter"]);
    }
    assert_eq!(report.added_flags, vec!["new_flag"]);
    p.flags.remove("kept_letter");
    assert!(load(&p, s).is_err());
}
#[test]
fn malformed_and_cross_project_saves_are_rejected() {
    let (p, _, s) = save();
    let mut bad = s.clone();
    bad.project_id = "other".into();
    assert!(load(&p, bad).is_err());
    bad = s.clone();
    bad.version = 99;
    assert!(load(&p, bad).is_err());
    bad = s.clone();
    bad.compatibility = None;
    assert!(load(&p, bad).is_err());
    bad = s.clone();
    bad.compatibility.as_mut().unwrap().nodes.remove("arrival");
    assert!(load(&p, bad).is_err());
    bad = s;
    bad.runtime.history = vec![bad.runtime.current.clone(); 501];
    assert!(load(&p, bad).is_err());
}
#[test]
fn exported_media_rebasing_keeps_v3_saves_loadable() {
    let (p, r, s) = save();
    let root = scratch();
    let exe = root.join("engine.exe");
    std::fs::write(&exe, b"test").unwrap();
    let output = root.join("game");
    export_game(
        &p,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/first-light"),
        &exe,
        &output,
    )
    .unwrap();
    assert_eq!(
        load(&read_project(&output.join("game.json")).unwrap(), s)
            .unwrap()
            .0,
        r
    );
}
#[test]
fn templates_and_forks_have_independent_ids_and_persistent_tasks() {
    let root = scratch();
    let a = create_project(&root, "作品甲", ProjectTemplate::Blank).unwrap();
    let b = create_project(&root, "作品乙", ProjectTemplate::Branching).unwrap();
    let mut pa = read_project(&a).unwrap();
    let pb = read_project(&b).unwrap();
    assert_ne!(pa.id, pb.id);
    assert!(
        validate(&pa, None)
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    assert!(
        validate(&pb, None)
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    pa.tasks.push(CreativeTask {
        id: unique_id(),
        title: "完成首章".into(),
        notes: "检查分支".into(),
        status: TaskStatus::Doing,
    });
    save_project(&a, &pa).unwrap();
    let fork = fork_project(&pa, a.parent().unwrap(), &root, "作品甲副本").unwrap();
    let pc = read_project(&fork).unwrap();
    assert_ne!(pc.id, pa.id);
    assert_eq!(pc.tasks, pa.tasks);
    assert_eq!(read_project(&a).unwrap(), pa);
    assert!(read_project(&b).unwrap().tasks.is_empty());
    assert!(load(&pc, Runtime::new(&pa).save(&pa)).is_err());
}
#[test]
fn catalog_deduplicates_paths_preserves_pins_and_counts_task_progress() {
    let root = scratch();
    let path = create_project(&root, "目录作品", ProjectTemplate::Blank).unwrap();
    let mut p = read_project(&path).unwrap();
    let mut c = WorkspaceCatalog::default();
    let i = c.register(&path, &p);
    c.projects[i].pinned = true;
    c.projects[i].stage = ProjectStage::Production;
    p.tasks.push(CreativeTask {
        id: unique_id(),
        title: "终稿".into(),
        notes: String::new(),
        status: TaskStatus::Done,
    });
    c.register(&path.parent().unwrap().join("./project.json"), &p);
    assert_eq!(c.projects.len(), 1);
    assert!(c.projects[0].pinned);
    assert_eq!(c.projects[0].tasks_done, 1);
    assert_eq!(c.projects[0].stage, ProjectStage::Production);
    let round: WorkspaceCatalog = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(round.projects[0].tasks_total, 1);
}
#[test]
fn large_project_snapshots_share_unchanged_nodes_and_index_survives_edits() {
    let mut p = template_project("长篇", ProjectTemplate::Blank);
    p.nodes = (0..5000)
        .map(|i| {
            let mut n = Node::new(format!("n{i}"), NodeKind::Dialogue, i);
            n.text = "中文长篇内容".repeat(80);
            n.next = (i < 4999).then(|| format!("n{}", i + 1));
            if i == 4999 {
                n.kind = NodeKind::Ending;
            }
            n
        })
        .collect();
    p.start = "n0".into();
    let before = p.clone();
    let mut history = vec![];
    for i in 0..100 {
        history.push(p.clone());
        p.nodes[i].text.push('新');
    }
    assert_eq!(before.nodes.shared_with(&p.nodes), 4900);
    assert_eq!(history[99].nodes.shared_with(&p.nodes), 4999);
    assert_eq!(p.node("n4999").unwrap().kind, NodeKind::Ending);
    p.nodes[4999].id = "renamed".into();
    assert!(p.node("n4999").is_none());
    assert!(p.node("renamed").is_some());
    p.nodes.swap(0, 4999);
    assert_eq!(p.node("renamed").unwrap().id, "renamed");
    p.nodes.remove(0);
    assert!(p.node("renamed").is_none());
    let json = serde_json::to_value(&before).unwrap();
    assert!(json["nodes"].is_array());
    assert_eq!(serde_json::from_value::<Project>(json).unwrap(), before);
}
#[test]
fn large_graph_scc_is_iterative_and_import_allocates_unique_ids() {
    let mut p = template_project("长篇", ProjectTemplate::Blank);
    let source = (0..2000)
        .map(|i| format!("旁白：第 {i} 段内容"))
        .collect::<Vec<_>>()
        .join("\n");
    authoring::import_dialogue(&mut p, &source, 0, "第一章").unwrap();
    authoring::import_dialogue(&mut p, &source, 0, "第二章").unwrap();
    assert_eq!(p.nodes.len(), 4002);
    let ids: BTreeSet<_> = p.nodes.iter().map(|n| &n.id).collect();
    assert_eq!(ids.len(), 4002);
    assert!(authoring::cycle_nodes(&p).is_empty());
    assert!(validate(&p, None).is_empty());
    let end = p
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Ending)
        .unwrap();
    p.nodes[end].kind = NodeKind::Dialogue;
    p.nodes[end].next = Some(p.start.clone());
    assert_eq!(authoring::cycle_nodes(&p).len(), 4002);
}
