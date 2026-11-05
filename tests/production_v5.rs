use opengalgame::{
    assets::{self, AssetKind},
    debugger::{Command, Route},
    logic::{Effect, Value},
    model::*,
    production::*,
    project_io::*,
    runtime::*,
    spreadsheet, testing,
};
use std::{collections::BTreeSet, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn demo() -> Project {
    serde_json::from_str(include_str!("../examples/tidal-stage/project.json")).unwrap()
}
fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(opengalgame::catalog::unique_id());
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn add(variable: &str, amount: i64) -> Effect {
    Effect::Add {
        variable: variable.into(),
        amount,
    }
}
fn short() -> Project {
    let mut p = demo();
    p.production.cases.clear();
    p.nodes = vec![
        Node::new("a".into(), NodeKind::Dialogue, 0),
        Node::new("b".into(), NodeKind::Ending, 1),
    ]
    .into();
    p.start = "a".into();
    p.nodes[0].next = Some("b".into());
    p
}
#[test]
fn entry_exit_and_destination_assignment_commit_in_order_and_back_never_repeats_entry() {
    let mut p = short();
    p.nodes[0].direction.enter = vec![add("visits", 1)];
    p.nodes[0].direction.exit = vec![add("visits", 2)];
    p.nodes[1].direction.enter = vec![add("visits", 4)];
    let mut r = Runtime::new(&p);
    assert_eq!(r.current.variables["visits"], Value::Number(1));
    let first = r.current.clone();
    r.advance(&p, None).unwrap();
    assert_eq!(r.current.variables["visits"], Value::Number(7));
    assert!(r.back());
    assert_eq!(r.current, first);
    r.advance(&p, None).unwrap();
    assert_eq!(r.current.variables["visits"], Value::Number(7));
}
#[test]
fn invalid_destination_entry_and_exit_overflow_roll_back_everything() {
    let mut p = short();
    p.nodes[0].direction.exit = vec![add("visits", 2)];
    p.nodes[1].direction.enter = vec![add("visits", i64::MAX)];
    let mut r = Runtime::new(&p);
    let before = r.clone();
    assert!(r.advance(&p, None).is_err());
    assert_eq!(r, before);
    p.nodes[0].direction.exit = vec![Effect::Set {
        variable: "visits".into(),
        value: Value::Text("wrong".into()),
    }];
    assert!(r.advance(&p, None).is_err());
    assert_eq!(r, before);
    p.nodes[0].direction.enter = vec![add("missing", 1)];
    let bad = Runtime::new(&p);
    assert!(bad.error.is_some());
    assert!(bad.visible_choices(&p).is_empty());
}
#[test]
fn hidden_choice_is_checked_before_exit_effects_and_jump_only_applies_new_entry() {
    let mut p = short();
    p.nodes[0].kind = NodeKind::Choice;
    p.nodes[0].choices = vec![Choice {
        text: "go".into(),
        target: "b".into(),
        condition: None,
        set_flag: None,
        when: None,
        effects: vec![add("visits", 4)],
    }];
    p.nodes[0].direction.exit = vec![add("visits", 2)];
    p.nodes[1].direction.enter = vec![add("visits", 8)];
    let mut r = Runtime::new(&p);
    r.advance(&p, Some(0)).unwrap();
    assert_eq!(r.current.variables["visits"], Value::Number(14));
    let mut r = Runtime::new(&p);
    Command::Jump { node: "b".into() }
        .apply(&p, &mut r)
        .unwrap();
    assert_eq!(r.current.variables["visits"], Value::Number(8));
    assert!(r.back());
    assert_eq!(r.current.variables["visits"], Value::Number(0));
}
#[test]
fn stage_sampling_interpolates_show_move_hide_and_preserves_wait_with_reduced_motion() {
    let mut n = Node::new("stage".into(), NodeKind::Ending, 0);
    n.direction.actions = vec![
        StageAction::Show {
            character: "qiao".into(),
            expression: None,
            side: PortraitSide::Left,
            scale: 1.,
            seconds: 1.,
        },
        StageAction::Move {
            character: "qiao".into(),
            side: PortraitSide::Right,
            scale: 1.4,
            seconds: 1.,
        },
        StageAction::Hide {
            character: "qiao".into(),
            seconds: 1.,
        },
        StageAction::Wait { seconds: 2. },
    ];
    let start = StageState::default();
    let frame = sample(&start, &n, 0.5, false);
    assert!((frame.actors[0].alpha - 0.5).abs() < 0.01);
    let frame = sample(&start, &n, 1.5, false);
    assert!((frame.actors[0].position - 0.5).abs() < 0.01);
    assert!((frame.actors[0].actor.scale - 1.2).abs() < 0.01);
    assert!((sample(&start, &n, 2.5, false).actors[0].alpha - 0.5).abs() < 0.01);
    assert!(!sample(&start, &n, 4.9, false).complete);
    assert!(sample(&start, &n, 5., false).complete);
    assert!(!sample(&start, &n, 1., true).complete);
    assert!(sample(&start, &n, 2., true).complete);
}
#[test]
fn final_stage_and_background_persist_across_nodes_and_restore_with_history() {
    let mut p = short();
    p.nodes[0].background = Some("bay".into());
    p.nodes[0].direction.actions = vec![
        StageAction::Show {
            character: "qiao".into(),
            expression: None,
            side: PortraitSide::Left,
            scale: 1.,
            seconds: 0.4,
        },
        StageAction::Move {
            character: "qiao".into(),
            side: PortraitSide::Right,
            scale: 1.2,
            seconds: 0.5,
        },
    ];
    p.nodes[1].character = Some("qiao".into());
    let mut r = Runtime::new(&p);
    let first = r.current.clone();
    r.advance(&p, None).unwrap();
    let stage = r.current.stage.as_ref().unwrap();
    assert_eq!(stage.actors[0].side, PortraitSide::Right);
    assert_eq!(stage.background.as_deref(), Some("bay"));
    assert!(r.back());
    assert_eq!(r.current, first);
}
#[test]
fn stage_and_voice_configuration_roundtrip_without_reexecuting_variables() {
    let p = demo();
    let mut r = Runtime::new(&p);
    r.advance(&p, None).unwrap();
    let save = r.save(&p);
    assert_eq!(save.version, 4);
    let loaded = Runtime::load(
        &p,
        serde_json::from_slice(&serde_json::to_vec(&save).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(loaded, r);
    assert_eq!(loaded.current.variables["visits"], Value::Number(1));
}
#[test]
fn genuine_v04_save_and_route_still_load_in_the_new_runtime() {
    let p = read_project(&root().join("examples/harbor-signals/project.json")).unwrap();
    let old: SaveGame = serde_json::from_str(include_str!("fixtures/v0.4-save.json")).unwrap();
    assert_eq!(old.version, 3);
    assert_eq!(Runtime::load(&p, old).unwrap().current.node, "return");
    let route: Route = serde_json::from_str(include_str!(
        "../examples/harbor-signals/route-promise.json"
    ))
    .unwrap();
    assert_eq!(route.replay(&p).unwrap().current.node, "ending_promise");
}
#[test]
fn visited_entry_effect_changes_reject_saves_but_cosmetic_actions_can_update() {
    let p = demo();
    let r = Runtime::new(&p);
    let save = r.save(&p);
    let mut changed = p.clone();
    changed.nodes[0].direction.enter.push(add("visits", 1));
    assert!(Runtime::load(&changed, save).is_err());
    let mut changed = p.clone();
    changed.nodes[0].direction.actions.push(StageAction::Show {
        character: "lin".into(),
        expression: None,
        side: PortraitSide::Left,
        scale: 1.,
        seconds: 1.,
    });
    let loaded = Runtime::load(&changed, r.save(&p)).unwrap();
    assert!(
        loaded
            .current
            .stage
            .unwrap()
            .actors
            .iter()
            .any(|a| a.character == "lin")
    );
}
#[test]
fn corrupted_stage_references_and_nonfinite_direction_parameters_are_rejected() {
    let p = demo();
    let r = Runtime::new(&p);
    let mut save = r.save(&p);
    save.runtime.current.stage.as_mut().unwrap().background = Some("missing".into());
    assert!(Runtime::load(&p, save).is_err());
    let mut p = p;
    p.nodes[0]
        .direction
        .actions
        .push(StageAction::Wait { seconds: f32::NAN });
    assert!(
        validate(&p, None)
            .iter()
            .any(|i| i.severity == Severity::Error)
    );
    let t = GameTheme {
        panel_width: f32::NAN,
        ..Default::default()
    };
    assert!(t.validate().is_err());
}
#[test]
fn material_identity_renames_cover_direction_and_saved_stage_aliases() {
    let p = demo();
    let mut r = Runtime::new(&p);
    r.advance(&p, None).unwrap();
    let save = r.save(&p);
    let renamed = assets::rename_id(&p, AssetKind::Character, "qiao", "qiao_new").unwrap();
    assert!(
        renamed
            .nodes
            .iter()
            .flat_map(|n| &n.direction.actions)
            .any(|a| matches!(a,StageAction::Show{character,..} if character=="qiao_new"))
    );
    let loaded = Runtime::load(&renamed, save).unwrap();
    assert!(
        loaded
            .current
            .stage
            .unwrap()
            .actors
            .iter()
            .any(|a| a.character == "qiao_new")
    );
    assert!(
        renamed
            .production
            .cases
            .iter()
            .all(|c| c.run(&renamed).is_ok())
    );
    let renamed = assets::rename_id(&renamed, AssetKind::Background, "bay", "bay_new").unwrap();
    let renamed = assets::rename_id(&renamed, AssetKind::Audio, "amb_waves", "amb_new").unwrap();
    assert!(
        validate(&renamed, None)
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    assert!(
        renamed
            .production
            .cases
            .iter()
            .all(|c| c.run(&renamed).is_ok())
    );
}
#[test]
fn identity_collisions_and_reserved_aliases_are_transactional_and_can_be_renamed_back() {
    let p = demo();
    let before = serde_json::to_vec(&p).unwrap();
    assert!(assets::rename_id(&p, AssetKind::Character, "qiao", "lin").is_err());
    assert_eq!(serde_json::to_vec(&p).unwrap(), before);
    let p = assets::rename_id(&p, AssetKind::Character, "qiao", "qiao_new").unwrap();
    assert!(assets::rename_id(&p, AssetKind::Character, "lin", "qiao").is_err());
    let p = assets::rename_id(&p, AssetKind::Character, "qiao_new", "qiao").unwrap();
    assert_eq!(
        assets::resolve_id(&p, AssetKind::Character, "qiao_new"),
        "qiao"
    );
}
#[test]
fn copied_filename_updates_all_references_while_preserving_the_old_file_for_undo() {
    let p = demo();
    let base = scratch();
    let p = clone_project(
        &p,
        &root().join("examples/tidal-stage"),
        &base.join("project.json"),
    )
    .unwrap();
    let old = p.backgrounds[0].file.clone();
    let renamed = assets::rename_file(&p, &base, &old, "新海湾").unwrap();
    assert!(safe_asset(&base, &old).is_ok());
    assert!(safe_asset(&base, &renamed.backgrounds[0].file).is_ok());
    assert_ne!(old, renamed.backgrounds[0].file);
    assert!(assets::rename_file(&p, &base, &old, "../outside").is_err());
    assert!(assets::rename_file(&p, &base, &old, "新海湾").is_err());
}
#[test]
fn batch_repair_requires_unique_names_and_rechecks_project_identity() {
    let mut p = demo();
    let base = scratch();
    let folder = scratch();
    let source = root().join("examples/tidal-stage/assets/bay.webp");
    std::fs::copy(&source, folder.join("lost.webp")).unwrap();
    let old = p.backgrounds[0].file.clone();
    assets::replace_path(&mut p, &old, "assets/lost.webp");
    let plan = assets::preview_repair(&p, &base, &folder).unwrap();
    assert_eq!(plan.replacements.len(), 1);
    let fixed = assets::apply_repair(&p, &base, &plan).unwrap();
    assert!(safe_asset(&base, &fixed.backgrounds[0].file).is_ok());
    let mut changed = p.clone();
    changed.title.push('变');
    assert!(assets::apply_repair(&changed, &base, &plan).is_err());
    std::fs::create_dir(folder.join("duplicates")).unwrap();
    std::fs::copy(&source, folder.join("duplicates/lost.webp")).unwrap();
    let ambiguous = assets::preview_repair(&p, &base, &folder).unwrap();
    assert!(ambiguous.replacements.is_empty());
    assert!(ambiguous.unresolved.contains(&"assets/lost.webp".into()));
}
#[test]
fn narrated_demo_audio_decodes_and_voice_matching_detects_ambiguity() {
    use rodio::Source;
    let p = demo();
    let base = root().join("examples/tidal-stage");
    for a in &p.audio {
        let audio =
            rodio::Decoder::try_from(std::fs::File::open(base.join(&a.file)).unwrap()).unwrap();
        assert!(audio.total_duration().is_some_and(|d| d.as_millis() > 100));
    }
    let (matched, ambiguous) = assets::voice_matches(&p, &base.join("assets")).unwrap();
    assert_eq!(matched.len(), 13);
    assert!(ambiguous.is_empty());
    let target = scratch();
    let assigned = assets::apply_voices(&p, &target, &matched).unwrap();
    assert!(assigned.nodes.iter().all(|n| n.direction.voice.is_some()));
    let folder = scratch();
    std::fs::copy(base.join("assets/opening.wav"), folder.join("opening.wav")).unwrap();
    std::fs::copy(base.join("assets/opening.wav"), folder.join("opening.mp3")).unwrap();
    let (matched, ambiguous) = assets::voice_matches(&p, &folder).unwrap();
    assert!(matched.is_empty());
    assert_eq!(ambiguous, vec!["opening"]);
}
#[test]
fn asset_audit_reports_unreferenced_disk_files_and_real_export_size() {
    let p = demo();
    let base = scratch();
    let p = clone_project(
        &p,
        &root().join("examples/tidal-stage"),
        &base.join("project.json"),
    )
    .unwrap();
    std::fs::write(base.join("assets/unused.bin"), [0u8; 17]).unwrap();
    let audit = assets::audit(&p, &base).unwrap();
    assert_eq!(audit.unused_bytes, 17);
    assert!(audit.referenced_bytes > 100000);
    assert!(audit.files.iter().all(|f| !f.missing));
    assert_eq!(audit.files.len(), assets::catalog_files(&p).len());
}
#[test]
fn custom_font_import_validation_clone_and_rebasing_are_complete() {
    let font = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()))
        .join("Fonts/arial.ttf");
    assert!(font.is_file());
    let base = scratch();
    let file = assets::import_font(&base, &font).unwrap();
    let mut p = short();
    p.characters.clear();
    p.backgrounds.clear();
    p.audio.clear();
    p.presentation.bgm = None;
    p.production.theme.as_mut().unwrap().font_family = "自定义".into();
    p.production.theme.as_mut().unwrap().font_file = Some(file);
    let target = scratch();
    let copy = clone_project(&p, &base, &target.join("project.json")).unwrap();
    assert!(
        safe_asset(
            &target,
            copy.production
                .theme
                .as_ref()
                .unwrap()
                .font_file
                .as_ref()
                .unwrap()
        )
        .is_ok()
    );
    let fake = base.join("bad.ttf");
    std::fs::write(&fake, b"invalid font").unwrap();
    assert!(assets::import_font(&base, &fake).is_err());
}
#[test]
fn automatic_coverage_and_saved_cases_reach_every_demo_ending_and_detect_regressions() {
    let p = demo();
    let report = testing::run(&p, 8192);
    assert!(report.passed(), "{:?}", report.errors);
    assert!(!report.truncated);
    assert_eq!(report.nodes.len(), 13);
    assert_eq!(report.endings.len(), 4);
    assert_eq!(report.cases.len(), 4);
    assert!(report.cases.iter().all(|c| c.passed));
    let mut changed = p.clone();
    changed.nodes[0].direction.enter.push(add("visits", 1));
    assert!(testing::run(&changed, 8192).cases.iter().any(|c| !c.passed));
    let mut renamed = p.clone();
    renamed.rename_node("ending_home", "ending_new").unwrap();
    assert!(
        renamed
            .production
            .cases
            .iter()
            .all(|c| c.run(&renamed).is_ok())
    );
}
#[test]
fn exploration_is_bounded_and_distinguishes_uncovered_paths_from_failures() {
    let mut p = short();
    p.nodes[0].next = Some("a".into());
    p.nodes[0].direction.enter = vec![add("visits", 1)];
    let report = testing::run(&p, 3);
    assert_eq!(report.states, 3);
    assert!(report.truncated);
    assert!(report.uncovered_endings.contains(&"b".into()));
    assert!(report.errors.is_empty());
    p.nodes[0].kind = NodeKind::Choice;
    p.nodes[0].choices.clear();
    let report = testing::run(&p, 5);
    assert!(!report.errors.is_empty());
}
#[test]
fn clipboard_grid_is_quoted_transactional_and_preserves_rich_metadata_and_ids() {
    let mut p = demo();
    let ids = vec![p.nodes[0].id.clone(), p.nodes[1].id.clone()];
    p.nodes[0].text = "逗号,制表\t符\n引号\"结束".into();
    let tsv = spreadsheet::copy_grid(&p, &ids).unwrap();
    let rows = spreadsheet::parse_delimited(&tsv, '\t').unwrap();
    assert_eq!(rows[0][4], p.nodes[0].text);
    let pasted = spreadsheet::paste_grid(&p, &ids, 0, &tsv).unwrap();
    assert_eq!(pasted, p);
    let changed = spreadsheet::paste_grid(&p, &ids, 2, "\"标题一\"\r\n\"标题二\"\r\n").unwrap();
    assert_eq!(changed.nodes[0].title, "标题一");
    assert_eq!(changed.nodes[1].title, "标题二");
    assert_eq!(changed.nodes[0].direction, p.nodes[0].direction);
    assert_eq!(changed.nodes[0].id, p.nodes[0].id);
    assert!(spreadsheet::paste_grid(&p, &ids, 6, "missing").is_err());
    assert_eq!(p.nodes[0].text, "逗号,制表\t符\n引号\"结束");
}
#[test]
fn csv_retains_new_direction_audio_theme_and_case_configuration() {
    let p = demo();
    let csv = spreadsheet::export(&p).unwrap();
    let preview = spreadsheet::preview(&p, &csv).unwrap();
    assert_eq!(preview.changed, 0);
    assert_eq!(preview.project, p);
    assert_eq!(preview.project.format_version, 3);
}
#[test]
fn exported_game_keeps_voice_theme_and_cases_playable_after_media_rebasing() {
    let p = demo();
    let destination = scratch().join("export");
    let exe = std::env::current_exe().unwrap();
    export_game(&p, &root().join("examples/tidal-stage"), &exe, &destination).unwrap();
    let copy = read_project(&destination.join("game.json")).unwrap();
    assert!(
        validate(&copy, Some(&destination))
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    assert!(testing::run(&copy, 8192).passed());
    assert!(copy.nodes.iter().all(|n| n.direction.voice.is_some()));
}
#[test]
fn envelopes_handle_starts_stops_and_immediate_levels_without_audio_hardware() {
    use opengalgame::sound::Envelope;
    let e = Envelope {
        from: 0.,
        to: 1.,
        seconds: 2.,
    };
    assert_eq!(e.at(0.), 0.);
    assert_eq!(e.at(1.), 0.5);
    assert_eq!(e.at(9.), 1.);
    assert_eq!(
        Envelope {
            from: 0.8,
            to: 0.,
            seconds: 2.
        }
        .at(1.),
        0.4
    );
    assert_eq!(
        Envelope {
            from: 0.,
            to: 0.7,
            seconds: 0.
        }
        .at(0.),
        0.7
    );
}
#[test]
fn all_nine_complete_demo_paths_survive_save_load_and_rollback() {
    fn walk(p: &Project, r: Runtime, depth: usize, endings: &mut BTreeSet<String>) -> usize {
        assert!(depth < 30);
        let restored = Runtime::load(p, r.save(p)).unwrap();
        assert_eq!(restored.current.variables, r.current.variables);
        let n = p.node(&r.current.node).unwrap();
        if n.kind == NodeKind::Ending {
            endings.insert(n.id.clone());
            return 1;
        }
        let choices = if n.kind == NodeKind::Choice {
            r.visible_choices(p)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>()
        } else {
            vec![None]
        };
        let mut total = 0;
        for choice in choices {
            let mut next = r.clone();
            next.advance(p, choice).unwrap();
            let mut back = next.clone();
            assert!(back.back());
            assert_eq!(back, r);
            total += walk(p, next, depth + 1, endings);
        }
        total
    }
    let p = demo();
    let mut endings = BTreeSet::new();
    assert_eq!(walk(&p, Runtime::new(&p), 0, &mut endings), 9);
    assert_eq!(endings.len(), 4);
}
