use opengalgame::{authoring::*, model::*, project_io::*, recovery, runtime::*};
use std::{collections::BTreeSet, path::PathBuf};

fn demo() -> Project {
    serde_json::from_str(include_str!("../examples/first-light/project.json")).unwrap()
}
fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("opengalgame-v2-{name}-{}", stamp()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn long_demo() -> (Project, PathBuf) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/long-form");
    (read_project(&base.join("project.json")).unwrap(), base)
}

#[test]
fn import_links_chapters_backgrounds_and_new_cast_without_losing_the_next_node() {
    let mut p = demo();
    let previous_next = p.nodes[0].next.clone();
    let bg = p.backgrounds[0].id.clone();
    let count = import_dialogue(
        &mut p,
        &format!("[第二章]\n@背景：{bg}\n小雨：你好。\n旁白：窗外放晴了。"),
        0,
        "第一章",
    )
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(p.nodes[0].next.as_deref(), Some(p.nodes[1].id.as_str()));
    assert_eq!(p.nodes[1].next.as_deref(), Some(p.nodes[2].id.as_str()));
    assert_eq!(p.nodes[2].next, previous_next);
    assert_eq!(p.nodes[1].chapter, "第二章");
    assert_eq!(p.nodes[1].background.as_deref(), Some(bg.as_str()));
    assert!(p.nodes[2].character.is_none());
    assert_eq!(
        p.characters
            .iter()
            .find(|c| Some(&c.id) == p.nodes[1].character.as_ref())
            .unwrap()
            .name,
        "小雨"
    );
    assert!(matches(&p.nodes[2], "放晴"));
    assert!(chapters(&p).contains(&"第二章".to_owned()));
}

#[test]
fn invalid_import_is_transactional_and_replacement_never_renames_identifiers() {
    let mut p = demo();
    let before = p.clone();
    assert!(import_dialogue(&mut p, "临时角色：你好\n@背景：找不到", 0, "章").is_err());
    assert_eq!(p, before);
    assert!(import_dialogue(&mut p, "[空章]", 0, "章").is_err());
    p.nodes[0].text = "arrival arrival".into();
    assert_eq!(replace_all(&mut p, "arrival", "归来"), 2);
    assert_eq!(p.start, "arrival");
    assert_eq!(p.nodes[0].id, "arrival");
    assert_eq!(replace_all(&mut p, "", "！"), 0);
}

#[test]
fn port_connections_and_cycle_detection_handle_choices_and_invalid_targets() {
    let mut p = demo();
    let port = Port {
        node: "arrival".into(),
        choice: None,
    };
    connect(&mut p, &port, "arrival").unwrap();
    assert!(cycle_nodes(&p).contains("arrival"));
    assert!(!cycle_nodes(&p).contains("train"));
    let before = p.clone();
    assert!(connect(&mut p, &port, "missing").is_err());
    assert_eq!(p, before);
    connect(
        &mut p,
        &Port {
            node: "letter".into(),
            choice: Some(0),
        },
        "train",
    )
    .unwrap();
    assert_eq!(p.node("letter").unwrap().choices[0].target, "train");
    assert!(
        connect(
            &mut p,
            &Port {
                node: "train".into(),
                choice: None
            },
            "arrival"
        )
        .is_err()
    );
}

#[test]
fn recovery_preserves_disk_and_requires_a_newer_matching_snapshot() {
    let disk = demo();
    let root = scratch("恢复");
    let path = root.join("故事.json");
    save_project(&path, &disk).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut edited = disk.clone();
    edited.nodes[0].text = "尚未手动保存的对白".into();
    recovery::checkpoint(&path, &disk, &edited).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let snapshot = recovery::candidate(&path, &disk).unwrap();
    assert_eq!(snapshot.project, edited);
    assert_eq!(snapshot.disk_fingerprint, fingerprint(&disk));
    let mut unrelated = disk.clone();
    unrelated.id = "different-project".into();
    assert!(recovery::candidate(&path, &unrelated).is_none());
    recovery::dismiss(&path, &disk).unwrap();
    assert!(recovery::candidate(&path, &disk).is_none());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

// Replay every complete choice path, round-tripping a save at each step. No path
// may silently lose flags, acquire hidden choices, or change after disk I/O.
fn routes(
    p: &Project,
) -> Vec<(
    String,
    std::collections::BTreeMap<String, bool>,
    Vec<String>,
)> {
    let mut pending = vec![Runtime::new(p)];
    let mut results = vec![];
    let mut steps = 0;
    while let Some(runtime) = pending.pop() {
        steps += 1;
        assert!(steps < 500, "unexpected cycle or path explosion");
        let bytes = serde_json::to_vec(&runtime.save(p)).unwrap();
        let runtime = Runtime::load(p, serde_json::from_slice(&bytes).unwrap()).unwrap();
        let node = p.node(&runtime.current.node).unwrap();
        match node.kind {
            NodeKind::Ending => {
                let mut trace: Vec<_> = runtime.history.iter().map(|s| s.node.clone()).collect();
                trace.push(runtime.current.node.clone());
                results.push((runtime.current.node, runtime.current.flags, trace));
            }
            NodeKind::Dialogue => {
                let mut next = runtime;
                next.advance(p, None).unwrap();
                pending.push(next);
            }
            NodeKind::Choice => {
                let choices = runtime.visible_choices(p);
                assert!(!choices.is_empty());
                for choice in choices {
                    let mut next = runtime.clone();
                    next.advance(p, Some(choice)).unwrap();
                    pending.push(next);
                }
            }
        }
    }
    results.sort();
    results
}

#[test]
fn forty_two_nodes_all_ten_paths_survive_edit_save_reopen_and_media_rebasing() {
    let (mut p, base) = long_demo();
    assert_eq!(p.nodes.len(), 42);
    p.nodes[0].text.push_str("\n这是一句保存后应保留的编辑。 ");
    let errors: Vec<_> = validate(&p, Some(&base))
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let expected = routes(&p);
    assert_eq!(expected.len(), 10);
    let endings: BTreeSet<_> = expected.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(endings, BTreeSet::from(["a10", "b10", "d10"]));
    let copy_path = scratch("迁移").join("长篇.json");
    let cloned = clone_project(&p, &base, &copy_path).unwrap();
    let reopened = read_project(&copy_path).unwrap();
    assert_eq!(cloned, reopened);
    assert_eq!(routes(&reopened), expected);
    assert!(
        validate(&reopened, copy_path.parent())
            .iter()
            .all(|i| i.severity != Severity::Error)
    );
    assert!(
        reopened
            .image_files()
            .iter()
            .all(|f| f.starts_with("assets/"))
    );
    for c in &reopened.characters {
        assert!(!c.expressions.is_empty());
    }
    assert_eq!(reopened.audio.len(), 2);
    assert!(reopened.presentation.cover.is_some());
    assert!(reopened.presentation.icon.is_some());
    for audio in &reopened.audio {
        let src = p.audio.iter().find(|a| a.id == audio.id).unwrap();
        assert_eq!(
            std::fs::read(base.join(&src.file)).unwrap(),
            std::fs::read(copy_path.parent().unwrap().join(&audio.file)).unwrap()
        );
    }
}

#[test]
fn validation_rejects_missing_audio_expressions_and_invalid_presentation() {
    let (mut p, _) = long_demo();
    p.presentation.bgm = Some("missing".into());
    p.nodes[0].sound = Some("missing-effect".into());
    p.nodes
        .iter_mut()
        .find(|n| !n.actors.is_empty())
        .unwrap()
        .actors[0]
        .expression = Some("不存在".into());
    p.presentation.font_size = f32::NAN;
    let errors: Vec<_> = validate(&p, None)
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.len() >= 4, "{errors:?}");
}

#[test]
fn audio_import_is_deduplicated_and_does_not_accept_fake_wav_files() {
    let (_, base) = long_demo();
    let target = scratch("audio");
    let source = base.join("assets/quiet-tide.wav");
    let a = import_audio(&target, &source).unwrap();
    let b = import_audio(&target, &source).unwrap();
    assert_eq!(a, b);
    let fake = target.join("fake.wav");
    std::fs::write(&fake, b"not audio").unwrap();
    assert!(import_audio(&target, &fake).is_err());
}

#[cfg(windows)]
#[test]
#[allow(clippy::manual_dangling_ptr)] // MAKEINTRESOURCE(1), not a Rust allocation.
fn actual_pe_export_contains_the_icon_and_all_ten_playable_paths() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{Foundation::FreeLibrary, System::LibraryLoader::*};
    let (p, base) = long_demo();
    let output = scratch("PE-export").join("game");
    export_game(&p, &base, &std::env::current_exe().unwrap(), &output).unwrap();
    let copy = read_project(&output.join("game.json")).unwrap();
    assert_eq!(routes(&copy), routes(&p));
    let exe: Vec<_> = output
        .join("Game.exe")
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Map only as data: inspect exported resources without executing the test EXE.
    unsafe {
        let module = LoadLibraryExW(exe.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_DATAFILE);
        assert!(!module.is_null());
        let icon = FindResourceW(module, 1usize as *const u16, 3usize as *const u16);
        let group = FindResourceW(module, 1usize as *const u16, 14usize as *const u16);
        assert!(!icon.is_null() && !group.is_null());
        let resource = LoadResource(module, icon);
        let data = std::slice::from_raw_parts(
            LockResource(resource).cast::<u8>(),
            SizeofResource(module, icon) as usize,
        );
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
        let image = image::load_from_memory(data).unwrap();
        assert_eq!((image.width(), image.height()), (256, 256));
        let resource = LoadResource(module, group);
        let data = std::slice::from_raw_parts(
            LockResource(resource).cast::<u8>(),
            SizeofResource(module, group) as usize,
        );
        assert_eq!(u16::from_le_bytes([data[18], data[19]]), 1);
        assert_ne!(FreeLibrary(module), 0);
    }
}
