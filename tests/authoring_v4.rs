use opengalgame::{
    catalog::*,
    compatibility,
    debugger::{Command, Debugger, Route},
    logic::*,
    model::*,
    project_io::*,
    runtime::*,
    spreadsheet,
};
use std::{collections::BTreeSet, path::PathBuf};
fn demo() -> Project {
    serde_json::from_str(include_str!("../examples/harbor-signals/project.json")).unwrap()
}
fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(unique_id());
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn condition(key: &str, op: Compare, value: Value) -> Condition {
    Condition::Compare {
        variable: key.into(),
        op,
        value,
    }
}
#[test]
fn integer_comparisons_and_text_boolean_equality_are_strict() {
    let p = demo();
    let r = Runtime::new(&p);
    for (op, expected) in [
        (Compare::Eq, true),
        (Compare::Ne, false),
        (Compare::Gt, false),
        (Compare::Ge, true),
        (Compare::Lt, false),
        (Compare::Le, true),
    ] {
        assert_eq!(
            condition("affinity", op, Value::Number(0))
                .evaluate(&r.current)
                .unwrap(),
            expected
        );
    }
    assert!(
        condition("signature", Compare::Eq, Value::Text("旅人".into()))
            .evaluate(&r.current)
            .unwrap()
    );
    assert!(
        condition("read_letter", Compare::Eq, Value::Bool(false))
            .evaluate(&r.current)
            .unwrap()
    );
    assert!(
        condition("affinity", Compare::Eq, Value::Text("0".into()))
            .validate(&p)
            .is_err()
    );
    assert!(
        condition("signature", Compare::Gt, Value::Text("a".into()))
            .validate(&p)
            .is_err()
    );
}
#[test]
fn nested_all_any_and_empty_or_excessively_deep_conditions() {
    let p = demo();
    let r = Runtime::new(&p);
    let yes = condition("affinity", Compare::Eq, Value::Number(0));
    let no = condition("clues", Compare::Gt, Value::Number(9));
    let c = Condition::All {
        items: vec![
            yes.clone(),
            Condition::Any {
                items: vec![no.clone(), yes.clone()],
            },
        ],
    };
    c.validate(&p).unwrap();
    assert!(c.evaluate(&r.current).unwrap());
    assert!(
        !Condition::All {
            items: vec![yes.clone(), no]
        }
        .evaluate(&r.current)
        .unwrap()
    );
    assert!(Condition::Any { items: vec![] }.validate(&p).is_err());
    let mut deep = yes;
    for _ in 0..10 {
        deep = Condition::All { items: vec![deep] };
    }
    assert!(deep.validate(&p).is_err());
    assert!(deep.evaluate(&r.current).is_err());
}
#[test]
fn numerical_effects_and_back_restore_all_types_together() {
    let p = demo();
    let mut r = Runtime::at(&p, "meeting");
    let old = r.clone();
    r.advance(&p, Some(0)).unwrap();
    assert_eq!(get(&r.current, "affinity"), Some(Value::Number(30)));
    assert_eq!(
        get(&r.current, "signature"),
        Some(Value::Text("归乡人".into()))
    );
    assert_eq!(get(&r.current, "promised"), Some(Value::Bool(true)));
    assert!(r.back());
    assert_eq!(r, old);
}
#[test]
fn overflow_invalid_type_and_missing_destination_are_transactional() {
    let mut p = demo();
    let i = p.nodes.iter().position(|n| n.id == "meeting").unwrap();
    p.variables
        .insert("affinity".into(), Value::Number(i64::MAX));
    let mut r = Runtime::at(&p, "meeting");
    let before = r.clone();
    assert!(r.advance(&p, Some(0)).is_err());
    assert_eq!(r, before);
    p.nodes[i].choices[0].effects = vec![Effect::Set {
        variable: "signature".into(),
        value: Value::Number(3),
    }];
    assert!(r.advance(&p, Some(0)).is_err());
    assert_eq!(r, before);
    p.nodes[i].choices[0].effects.clear();
    p.nodes[i].choices[0].target = "missing".into();
    assert!(r.advance(&p, Some(0)).is_err());
    assert_eq!(r, before);
}
#[test]
fn legacy_assignment_executes_before_typed_effects() {
    let mut p = demo();
    let i = p.nodes.iter().position(|n| n.id == "letter").unwrap();
    p.nodes[i].choices[0].set_flag = Some(FlagValue {
        flag: "read_letter".into(),
        value: false,
    });
    let mut r = Runtime::at(&p, "letter");
    r.advance(&p, Some(0)).unwrap();
    assert!(r.current.flags["read_letter"]);
}
#[test]
fn hidden_option_reports_actual_values_and_cannot_be_executed() {
    let p = demo();
    let mut r = Runtime::at(&p, "decision");
    let choice = &p.node("decision").unwrap().choices[0];
    let reason = choice_reason(choice, &r.current);
    assert!(reason.contains("affinity") && reason.contains("50"));
    assert!(!r.visible_choices(&p).contains(&0));
    let before = r.clone();
    assert!(r.advance(&p, Some(0)).is_err());
    assert_eq!(r, before);
}
#[test]
fn sample_has_four_cast_three_chapters_twelve_paths_and_five_reachable_endings() {
    let p = demo();
    assert_eq!(p.characters.len(), 4);
    assert_eq!(opengalgame::authoring::chapters(&p).len(), 3);
    assert!(
        validate(
            &p,
            Some(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/harbor-signals"))
        )
        .is_empty()
    );
    let mut queue = vec![Runtime::new(&p)];
    let mut ends = BTreeSet::new();
    let mut paths = 0;
    while let Some(r) = queue.pop() {
        let n = p.node(&r.current.node).unwrap();
        if n.kind == NodeKind::Ending {
            ends.insert(n.id.clone());
            paths += 1;
            continue;
        }
        let actions = if n.kind == NodeKind::Choice {
            r.visible_choices(&p).into_iter().map(Some).collect()
        } else {
            vec![None]
        };
        for action in actions {
            let mut next = r.clone();
            next.advance(&p, action).unwrap();
            let saved = next.save(&p);
            assert_eq!(Runtime::load(&p, saved).unwrap(), next);
            queue.push(next);
        }
    }
    assert_eq!(ends.len(), 5);
    assert_eq!(paths, 12);
}
#[test]
fn new_save_roundtrip_defaults_and_type_changes() {
    let mut p = demo();
    let r = Runtime::at(&p, "meeting");
    let save = r.save(&p);
    assert_eq!(save.version, 4);
    p.variables.insert("new_count".into(), Value::Number(8));
    let (restored, report) = compatibility::load(&p, save.clone()).unwrap();
    assert_eq!(get(&restored.current, "new_count"), Some(Value::Number(8)));
    assert!(report.added_flags.contains(&"new_count".into()));
    p.variables
        .insert("affinity".into(), Value::Text("0".into()));
    assert!(Runtime::load(&p, save).is_err());
}
#[test]
fn removed_missing_and_unschematized_saved_values_reject() {
    let p = demo();
    let save = Runtime::new(&p).save(&p);
    let mut bad = save.clone();
    bad.runtime.current.variables.remove("affinity");
    assert!(Runtime::load(&p, bad).is_err());
    let mut bad = save.clone();
    bad.compatibility.as_mut().unwrap().variables.clear();
    assert!(Runtime::load(&p, bad).is_err());
    let mut changed = p;
    changed.variables.remove("signature");
    assert!(Runtime::load(&changed, save).is_err());
}
#[test]
fn condition_or_effect_edits_on_visited_nodes_reject_saved_progress() {
    let mut p = demo();
    let r = Runtime::at(&p, "decision");
    let save = r.save(&p);
    let index = p.nodes.iter().position(|n| n.id == "decision").unwrap();
    p.nodes[index].choices[0].when = None;
    assert!(Runtime::load(&p, save).is_err());
    let save = Runtime::at(&p, "meeting").save(&p);
    let index = p.nodes.iter().position(|n| n.id == "meeting").unwrap();
    p.nodes[index].choices[0].effects.clear();
    assert!(Runtime::load(&p, save).is_err());
}
#[test]
fn genuine_v03_save_upgrades_and_new_defaults_fill_current_and_history() {
    let mut p: Project =
        serde_json::from_str(include_str!("../examples/first-light/project.json")).unwrap();
    let old: SaveGame = serde_json::from_str(include_str!("fixtures/v0.3-save.json")).unwrap();
    assert_eq!(old.version, 2);
    assert_eq!(old.fingerprint, fingerprint(&p));
    let r = Runtime::load(&p, old.clone()).unwrap();
    p.variables.insert("affinity".into(), Value::Number(12));
    p.format_version = 2;
    let migrated = Runtime::load(&p, old).unwrap();
    assert_eq!(migrated.current.node, r.current.node);
    for step in std::iter::once(&migrated.current).chain(&migrated.history) {
        assert_eq!(get(step, "affinity"), Some(Value::Number(12)));
    }
}
#[test]
fn version_one_projects_cannot_silently_save_typed_features() {
    let mut p = demo();
    p.format_version = 1;
    assert!(
        validate(&p, None)
            .iter()
            .any(|i| i.severity == Severity::Error)
    );
    let path = scratch().join("project.json");
    assert!(save_project(&path, &p).is_err());
    assert!(!path.exists());
}
#[test]
fn route_reproduces_choices_overrides_jump_and_rollback() {
    let p = demo();
    let mut r = Runtime::new(&p);
    let mut route = Route::new(&p, &r);
    for command in [
        Command::Advance { choice: None },
        Command::Advance { choice: Some(0) },
        Command::Set {
            variable: "affinity".into(),
            value: Value::Number(70),
        },
        Command::Jump {
            node: "decision".into(),
        },
        Command::Advance { choice: Some(2) },
        Command::Back,
    ] {
        command.apply(&p, &mut r).unwrap();
        route.record(command, &r).unwrap();
    }
    let copy: Route = serde_json::from_slice(&serde_json::to_vec(&route).unwrap()).unwrap();
    assert_eq!(copy.replay(&p).unwrap(), r);
    assert!(
        copy.replay_with_coverage(&p)
            .unwrap()
            .1
            .contains("ending_travel")
    );
}
#[test]
fn route_rejects_wrong_project_changed_content_and_wrong_expected_state() {
    let p = demo();
    let route = Route::new(&p, &Runtime::new(&p));
    let mut other = p.clone();
    other.id = "other".into();
    assert!(route.replay(&other).is_err());
    other = p.clone();
    other.nodes[0].text.push('!');
    assert!(route.replay(&other).is_err());
    let mut bad = route;
    bad.expected.current.node = "decision".into();
    assert!(bad.replay(&p).is_err());
}
#[test]
fn route_coverage_is_not_limited_to_five_hundred_rollback_steps() {
    let mut p = template_project("路线", ProjectTemplate::Blank);
    p.nodes = (0..700)
        .map(|i| {
            let mut n = Node::new(format!("n{i}"), NodeKind::Dialogue, i);
            if i == 699 {
                n.kind = NodeKind::Ending;
            } else {
                n.next = Some(format!("n{}", i + 1));
            }
            n
        })
        .collect();
    p.start = "n0".into();
    let mut r = Runtime::new(&p);
    let mut route = Route::new(&p, &r);
    for _ in 0..699 {
        let cmd = Command::Advance { choice: None };
        cmd.apply(&p, &mut r).unwrap();
        route.record(cmd, &r).unwrap();
    }
    assert_eq!(r.history.len(), 500);
    assert_eq!(route.replay_with_coverage(&p).unwrap().1.len(), 700);
}
#[test]
fn breakpoints_stop_continue_and_debuggers_are_per_project() {
    let p = demo();
    let mut a = Debugger::default();
    let b = Debugger::default();
    a.breakpoints.insert("meeting".into());
    let mut r = Runtime::new(&p);
    a.start(&p, &r);
    a.running = true;
    r.advance(&p, None).unwrap();
    a.observe(&p, &r);
    assert!(!a.running);
    assert!(a.message.contains("meeting"));
    assert!(b.breakpoints.is_empty() && b.visited.is_empty());
}
#[test]
fn csv_preserves_quotes_multiline_unicode_and_every_node_field() {
    let mut p = demo();
    p.nodes[0].text = "中文，逗号,\"引号\"\r\n第二行\n第三行".into();
    let csv = spreadsheet::export(&p).unwrap();
    let preview = spreadsheet::preview(&p, &format!("\u{feff}{csv}")).unwrap();
    assert_eq!(preview.project, p);
    assert_eq!(preview.changed, 0);
    assert_eq!(preview.added, 0);
}
#[test]
fn csv_import_updates_by_identity_and_preserves_unlisted_nodes() {
    let p = demo();
    let mut edit = p.clone();
    edit.nodes[0].title = "新的标题".into();
    edit.nodes[0].chapter = "新章节".into();
    let csv = spreadsheet::export(&edit).unwrap();
    let preview = spreadsheet::preview(&p, &csv).unwrap();
    assert_eq!(preview.changed, 1);
    assert_eq!(preview.project.nodes.len(), p.nodes.len());
    assert_eq!(preview.project.nodes[0].title, "新的标题");
    assert_eq!(p.nodes[0].title, "回到灯塔");
}
#[test]
fn csv_invalid_quotes_duplicate_ids_and_invalid_links_never_apply() {
    let p = demo();
    let before = p.clone();
    assert!(spreadsheet::parse("a,\"unclosed").is_err());
    assert!(spreadsheet::parse("a,\"done\"bad").is_err());
    let csv = spreadsheet::export(&p).unwrap();
    let lines = spreadsheet::parse(&csv).unwrap();
    assert_eq!(lines.len(), 15);
    let mut bad = p.clone();
    bad.nodes[0].next = Some("missing".into());
    assert!(spreadsheet::preview(&p, &spreadsheet::export(&bad).unwrap()).is_err());
    let first = spreadsheet::export(&template_project("简短", ProjectTemplate::Blank)).unwrap();
    let mut rows = first.lines();
    let header = rows.next().unwrap();
    let row = rows.next().unwrap();
    assert!(spreadsheet::preview(&p, &format!("{header}\r\n{row}\r\n{row}\r\n")).is_err());
    assert_eq!(p, before);
}
#[test]
fn csv_file_import_and_project_disk_roundtrip_keep_logic() {
    let p = demo();
    let root = scratch();
    let csv = root.join("中文剧本.csv");
    std::fs::write(&csv, spreadsheet::export(&p).unwrap()).unwrap();
    let preview = spreadsheet::read_csv(&csv, &p).unwrap();
    let path = root.join("project.json");
    save_project(&path, &preview.project).unwrap();
    assert_eq!(read_project(&path).unwrap(), p);
}
