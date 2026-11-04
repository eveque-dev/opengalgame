use opengalgame::{model::*, project_io::*, runtime::*};
use std::path::PathBuf;
fn demo() -> Project {
    serde_json::from_str(include_str!("../examples/first-light/project.json")).unwrap()
}
fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("opengalgame-test-{name}-{}", stamp()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn sample_validates_real_assets() {
    let p = demo();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/first-light");
    let errors: Vec<_> = validate(&p, Some(&base))
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
#[test]
fn branches_and_flag_conditions_produce_two_endings() {
    let p = demo();
    let mut r = Runtime::new(&p);
    r.advance(&p, None).unwrap();
    r.advance(&p, None).unwrap();
    assert!(r.advance(&p, None).is_err());
    assert!(r.advance(&p, Some(77)).is_err());
    let decision = r.clone();
    r.advance(&p, Some(0)).unwrap();
    assert!(r.current.flags["kept_letter"]);
    r.advance(&p, None).unwrap();
    assert_eq!(r.visible_choices(&p), vec![0]);
    r.advance(&p, Some(0)).unwrap();
    assert_eq!(r.current.node, "morning");
    assert!(r.advance(&p, None).is_err());
    let mut r = decision;
    r.advance(&p, Some(1)).unwrap();
    assert!(!r.current.flags["kept_letter"]);
    r.advance(&p, None).unwrap();
    assert_eq!(r.current.node, "train");
}
#[test]
fn back_rolls_back_choice_and_variable_together() {
    let p = demo();
    let mut r = Runtime::at(&p, "letter");
    let original = r.clone();
    r.advance(&p, Some(0)).unwrap();
    assert!(r.current.flags["kept_letter"]);
    assert!(r.back());
    assert_eq!(r, original);
    assert!(!r.back());
}
#[test]
fn hidden_choice_cannot_be_selected_even_through_api() {
    let p = demo();
    let mut r = Runtime::at(&p, "promise");
    assert!(r.visible_choices(&p).is_empty());
    let before = r.clone();
    assert!(r.advance(&p, Some(0)).is_err());
    assert_eq!(r, before);
}
#[test]
fn saves_round_trip_and_reject_stale_or_forged_states() {
    let p = demo();
    let mut r = Runtime::new(&p);
    r.advance(&p, None).unwrap();
    let save = r.save(&p);
    let bytes = serde_json::to_vec(&save).unwrap();
    assert_eq!(
        Runtime::load(&p, serde_json::from_slice(&bytes).unwrap()).unwrap(),
        r
    );
    let mut changed = p.clone();
    changed.nodes[0].text.push('!');
    assert!(Runtime::load(&changed, save.clone()).is_err());
    let mut bad = save.clone();
    bad.runtime.current.node = "missing".into();
    assert!(Runtime::load(&p, bad).is_err());
    let mut bad = save;
    bad.runtime.current.flags.insert("forged".into(), true);
    assert!(Runtime::load(&p, bad).is_err());
}
#[test]
fn rename_updates_start_and_all_incoming_links() {
    let mut p = demo();
    p.rename_node("arrival", "new_start").unwrap();
    assert_eq!(p.start, "new_start");
    p.rename_node("letter", "decision").unwrap();
    assert_eq!(p.node("reply").unwrap().next.as_deref(), Some("decision"));
    p.rename_node("morning", "new_end").unwrap();
    assert_eq!(p.node("promise").unwrap().choices[0].target, "new_end");
    assert!(p.rename_node("new_start", "decision").is_err());
}
#[test]
fn validation_catches_dangling_links_duplicates_and_conditional_deadlocks() {
    let mut p = demo();
    p.nodes[0].next = Some("missing".into());
    assert!(
        validate(&p, None)
            .iter()
            .any(|i| i.message.contains("missing"))
    );
    let mut p = demo();
    p.nodes.push(p.nodes[0].clone());
    assert!(
        validate(&p, None)
            .iter()
            .any(|i| i.message.contains("重复"))
    );
    let mut p = demo();
    p.nodes
        .iter_mut()
        .find(|n| n.id == "letter")
        .unwrap()
        .choices[0]
        .set_flag = None;
    assert!(
        validate(&p, None)
            .iter()
            .any(|i| i.message.contains("没有可选选项"))
    );
}
#[test]
fn asset_paths_cannot_escape_or_refer_to_absolute_files() {
    let base = scratch("paths");
    assert!(safe_asset(&base, "../secret.png").is_err());
    assert!(safe_asset(&base, r"..\secret.png").is_err());
    assert!(safe_asset(&base, r"C:\secret.png").is_err());
    assert!(safe_asset(&base, "/etc/passwd").is_err());
}
#[test]
fn export_copies_only_referenced_assets_and_rebases_paths() {
    let project = demo();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/first-light");
    let root = scratch("export");
    let fake_exe = root.join("engine.exe");
    std::fs::write(&fake_exe, b"test executable content").unwrap();
    let output = root.join("game");
    export_game(&project, &source, &fake_exe, &output).unwrap();
    let copy = read_project(&output.join("game.json")).unwrap();
    assert!(
        validate(&copy, Some(&output))
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    assert!(output.join("Game.exe").is_file());
    assert_eq!(std::fs::read_dir(output.join("assets")).unwrap().count(), 4);
    assert!(
        copy.backgrounds
            .iter()
            .all(|b| b.file.starts_with("assets/") && !b.file.contains("C:"))
    );
    assert!(export_game(&project, &source, &fake_exe, &output).is_err());
    let mut runtime = Runtime::new(&copy);
    runtime.advance(&copy, None).unwrap();
    assert_eq!(runtime.current.node, "reply");
}
#[test]
fn atomic_save_preserves_backup_and_accepts_chinese_path() {
    let root = scratch("中文路径");
    let file = root.join("雨停之前.json");
    let mut p = demo();
    save_project(&file, &p).unwrap();
    p.title = "新的名字".into();
    save_project(&file, &p).unwrap();
    assert_eq!(read_project(&file).unwrap().title, "新的名字");
    assert_eq!(
        read_project(&file.with_extension("json.bak"))
            .unwrap()
            .title,
        "雨停之前"
    );
}

#[test]
fn auto_layout_handles_cycles_and_disconnected_nodes() {
    let mut project = demo();
    let mut detached = Node::new("detached".into(), NodeKind::Dialogue, 9);
    detached.next = Some("detached".into());
    project.nodes.push(detached);
    arrange_nodes(&mut project);
    assert_eq!(project.nodes[0].position, [32., 32.]);
    assert!(
        project.node("letter").unwrap().position[0] > project.node("reply").unwrap().position[0]
    );
    assert!(
        project
            .nodes
            .iter()
            .all(|n| n.position.iter().all(|v| v.is_finite() && *v >= 0.))
    );
    let mut repeated = project.clone();
    arrange_nodes(&mut repeated);
    assert_eq!(project, repeated);
}

#[test]
fn portrait_layout_is_backward_compatible_and_validated() {
    let mut project = demo();
    assert_eq!(project.nodes[0].portrait_side, PortraitSide::Right);
    assert_eq!(project.nodes[0].portrait_scale, 1.0);
    project.nodes[0].portrait_side = PortraitSide::Left;
    project.nodes[0].portrait_scale = 0.8;
    let round: Project = serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert_eq!(round, project);
    project.nodes[0].portrait_scale = 20.;
    assert!(
        validate(&project, None)
            .iter()
            .any(|i| i.severity == Severity::Error && i.message.contains("立绘"))
    );
}
