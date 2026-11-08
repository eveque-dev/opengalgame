use opengalgame::{
    catalog::unique_id,
    model::{CharacterProfile, NodeKind, Project},
    project_io::{export_game, read_project, save_project},
    runtime::{Runtime, fingerprint},
    story_tools::{
        AuditPlan, BranchDraft, ContextPacket, INPUT_LIMIT, decode_json, selected_context,
    },
};
use std::{collections::BTreeSet, path::Path};

fn sample() -> Project {
    read_project(Path::new("examples/tidal-stage/project.json")).unwrap()
}

fn branch() -> BranchDraft {
    decode_json(
        r#"{"entry":"fork","scenes":[
          {"id":"fork","title":"新路线","kind":"choice","text":"下一步去哪里？",
           "choices":[{"text":"回家","target":"home"},{"text":"到码头","target":"pier"}]},
          {"id":"home","title":"归家","kind":"ending","text":"灯亮着。"},
          {"id":"pier","title":"码头","kind":"ending","text":"船离开了。"}
        ]}"#,
    )
    .unwrap()
}

#[test]
fn author_notes_save_with_project_and_require_explicit_selection() {
    let mut project = sample();
    let before = fingerprint(&project);
    project.story_recap = "此前两人在码头分开。".into();
    project.characters[0].profile = CharacterProfile {
        role: "保管剧本的人".into(),
        voice: "只说短句".into(),
        continuity: "第二幕前不知道信的内容".into(),
    };
    project.characters[1].profile.role = "未选中的设定".into();
    let chosen = BTreeSet::from([project.characters[0].id.clone()]);
    let nothing =
        serde_json::to_string(&selected_context(&project, false, &BTreeSet::new())).unwrap();
    assert_eq!(nothing, "{}");
    let packet = serde_json::to_string(&selected_context(&project, true, &chosen)).unwrap();
    assert!(packet.contains(&project.story_recap));
    assert!(packet.contains("只说短句"));
    assert!(!packet.contains("未选中的设定") && !packet.contains("portrait"));
    assert_eq!(fingerprint(&project), before);

    let temp = std::env::temp_dir().join(unique_id());
    let file = temp.join("project.json");
    save_project(&file, &project).unwrap();
    let reopened = read_project(&file).unwrap();
    assert_eq!(reopened.story_recap, project.story_recap);
    assert_eq!(
        reopened.characters[0].profile,
        project.characters[0].profile
    );
    let game = temp.join("game");
    export_game(
        &reopened,
        Path::new("examples/tidal-stage"),
        Path::new(env!("CARGO_BIN_EXE_opengalgame")),
        &game,
    )
    .unwrap();
    let exported = read_project(&game.join("game.json")).unwrap();
    assert!(exported.story_recap.is_empty());
    assert!(
        exported
            .characters
            .iter()
            .all(|c| c.profile == CharacterProfile::default())
    );
}

#[test]
fn edited_branch_rebases_ids_preserves_routes_and_plays_both_endings() {
    let mut project = sample();
    let previous = project.node("meeting").unwrap().choices.clone();
    project
        .save_compatibility
        .node_aliases
        .insert("ai_scene_1".into(), "meeting".into());
    let mut draft = branch();
    draft.scenes[0].title = "校对后的分支".into();
    let entry = draft
        .insert(&mut project, Some(("meeting", "打开新路线")))
        .unwrap();
    assert_eq!(entry, "ai_scene_2");
    assert_eq!(project.node(&entry).unwrap().title, "校对后的分支");
    assert_eq!(
        &project.node("meeting").unwrap().choices[..previous.len()],
        &previous
    );
    for option in 0..2 {
        let mut runtime = Runtime::at(&project, "meeting");
        runtime.advance(&project, Some(previous.len())).unwrap();
        assert_eq!(runtime.current.node, entry);
        runtime.advance(&project, Some(option)).unwrap();
        let end = project.node(&runtime.current.node).unwrap();
        assert_eq!(end.kind, NodeKind::Ending);
        assert_eq!(end.title, ["归家", "码头"][option]);
    }
}

#[test]
fn rejected_branches_leave_the_project_unchanged() {
    let mut project = sample();
    let original = project.clone();
    let mut draft = branch();
    draft.scenes[0].choices[0].target = "missing".into();
    assert!(draft.insert(&mut project, None).is_err());
    draft = branch();
    draft.scenes[1].character = Some("invented-character".into());
    assert!(draft.insert(&mut project, None).is_err());
    draft = branch();
    draft.scenes[1].kind = NodeKind::Dialogue;
    draft.scenes[1].next = Some("home".into());
    assert!(draft.insert(&mut project, None).is_err());
    draft = branch();
    assert!(
        draft
            .insert(&mut project, Some(("opening", "新增选项")))
            .is_err()
    );
    let duplicate = original.node("meeting").unwrap().choices[0].text.clone();
    assert!(
        draft
            .insert(&mut project, Some(("meeting", &duplicate)))
            .is_err()
    );
    assert_eq!(project, original);
}

#[test]
fn audit_batches_cover_the_complete_text_even_for_a_long_single_node() {
    let mut project = sample();
    let text = format!("长段开头{}长段结尾", "潮声🌊".repeat(8000));
    project.nodes[0].text = text.clone();
    project.story_recap = "不要发送的作者笔记".into();
    let plan = AuditPlan::prepare(&project, &ContextPacket::default()).unwrap();
    assert!(plan.len() > 1);
    assert_eq!(plan.node_ids.len(), project.nodes.len());
    let memo = "一条跨批摘要".repeat(1000);
    let mut body = String::new();
    for index in 0..plan.len() {
        let input = plan.input(index, &memo);
        assert!(input.len() <= INPUT_LIMIT);
        assert!(input.contains("一条跨批摘要"));
        assert!(!input.contains(&project.story_recap));
        let records = input
            .split_once("\n剧本批次 ")
            .unwrap()
            .1
            .split_once('\n')
            .unwrap()
            .1;
        for line in records.lines().filter(|line| !line.starts_with("记录 ")) {
            body.push_str(line);
        }
    }
    assert!(body.contains(&text));
    assert!(body.contains("\"id\":\"ending_truth\""));
}
