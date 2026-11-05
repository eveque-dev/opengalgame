//! Asset identities and disk files are changed transactionally; old files survive undo.
use crate::{model::*, production::*, project_io::*};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Character,
    Background,
    Audio,
    Font,
}
pub type VoiceMatches = Vec<(String, PathBuf)>;
pub type VoicePlan = (String, VoiceMatches, Vec<String>);
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetFile {
    pub file: String,
    pub kind: AssetKind,
    pub owners: Vec<String>,
    pub bytes: u64,
    pub missing: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Audit {
    pub files: Vec<AssetFile>,
    pub unused_disk: Vec<(String, u64)>,
    pub unused_catalog: Vec<String>,
    pub referenced_bytes: u64,
    pub unused_bytes: u64,
}
pub fn catalog_files(p: &Project) -> BTreeMap<String, (AssetKind, Vec<String>)> {
    let mut files = BTreeMap::<String, (AssetKind, Vec<String>)>::new();
    let mut add = |file: &str, kind, label: String| {
        files
            .entry(file.into())
            .or_insert((kind, vec![]))
            .1
            .push(label);
    };
    for b in &p.backgrounds {
        add(&b.file, AssetKind::Background, format!("背景 · {}", b.name));
    }
    for c in &p.characters {
        for file in c.portrait.iter().chain(c.expressions.values()) {
            add(file, AssetKind::Character, format!("人物 · {}", c.name));
        }
    }
    for a in &p.audio {
        add(&a.file, AssetKind::Audio, format!("音频 · {}", a.name));
    }
    for file in p
        .presentation
        .cover
        .iter()
        .chain(p.presentation.icon.iter())
    {
        add(file, AssetKind::Background, "封面 / 图标".into());
    }
    if let Some(theme) = &p.production.theme
        && let Some(file) = &theme.font_file
    {
        add(file, AssetKind::Font, "游戏字体".into());
    }
    files
}
fn walk(base: &Path) -> Result<Vec<PathBuf>> {
    ensure!(base.is_dir(), "请选择有效目录");
    let mut found = vec![];
    let mut pending = vec![(base.to_owned(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        ensure!(
            found.len() + pending.len() < 20000,
            "素材目录过大，请缩小搜索范围"
        );
        for entry in std::fs::read_dir(dir)? {
            let e = entry?;
            let kind = e.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_file() {
                found.push(e.path());
            } else if kind.is_dir() && depth < 8 {
                pending.push((e.path(), depth + 1));
            }
        }
    }
    Ok(found)
}
pub fn audit(p: &Project, base: &Path) -> Result<Audit> {
    let mut report = Audit::default();
    let files = catalog_files(p);
    for (file, (kind, owners)) in &files {
        let metadata = safe_asset(base, file).and_then(|path| Ok(std::fs::metadata(path)?));
        let bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        report.referenced_bytes += bytes;
        report.files.push(AssetFile {
            file: file.clone(),
            kind: *kind,
            owners: owners.clone(),
            bytes,
            missing: metadata.is_err(),
        });
    }
    let assets = base.join("assets");
    if assets.is_dir() {
        for file in walk(&assets)? {
            let relative = file
                .strip_prefix(base)?
                .to_string_lossy()
                .replace('\\', "/");
            if !files.contains_key(&relative) {
                let bytes = std::fs::metadata(&file)?.len();
                report.unused_bytes += bytes;
                report.unused_disk.push((relative, bytes));
            }
        }
    }
    for (kind, id) in p
        .characters
        .iter()
        .map(|c| ("人物", &c.id))
        .chain(p.backgrounds.iter().map(|b| ("背景", &b.id)))
        .chain(p.audio.iter().map(|a| ("音频", &a.id)))
    {
        if p.asset_use_count(id) == 0 && p.presentation.bgm.as_ref() != Some(id) {
            report.unused_catalog.push(format!("{kind} · {id}"));
        }
    }
    Ok(report)
}
pub fn replace_path(p: &mut Project, old: &str, new: &str) {
    for b in &mut p.backgrounds {
        if b.file == old {
            b.file = new.into();
        }
    }
    for c in &mut p.characters {
        for f in c.portrait.iter_mut().chain(c.expressions.values_mut()) {
            if f == old {
                *f = new.into();
            }
        }
    }
    for a in &mut p.audio {
        if a.file == old {
            a.file = new.into();
        }
    }
    for f in p
        .presentation
        .cover
        .iter_mut()
        .chain(p.presentation.icon.iter_mut())
    {
        if f == old {
            *f = new.into();
        }
    }
    if let Some(t) = &mut p.production.theme
        && t.font_file.as_deref() == Some(old)
    {
        t.font_file = Some(new.into());
    }
}
pub fn import_font(base: &Path, source: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    ensure!(
        std::fs::metadata(source)?.len() <= 30 * 1024 * 1024,
        "字体不能超过 30 MB"
    );
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    ensure!(
        ["ttf", "otf", "ttc"].contains(&ext.as_str()),
        "请选择 TTF、OTF 或 TTC 字体"
    );
    let bytes = std::fs::read(source)?;
    ab_glyph::FontRef::try_from_slice(&bytes).map_err(|_| anyhow::anyhow!("字体无法解析"))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let relative = format!("assets/{}.{ext}", &hash[..20]);
    std::fs::create_dir_all(base.join("assets"))?;
    if !base.join(&relative).exists() {
        std::fs::write(base.join(&relative), bytes)?;
    }
    Ok(relative)
}
pub fn import_by_kind(kind: AssetKind, base: &Path, source: &Path) -> Result<String> {
    match kind {
        AssetKind::Audio => import_audio(base, source),
        AssetKind::Font => import_font(base, source),
        _ => import_asset(base, source),
    }
}
pub fn rename_file(p: &Project, base: &Path, old: &str, stem: &str) -> Result<Project> {
    ensure!(
        !stem.trim().is_empty()
            && stem
                .chars()
                .all(|c| c.is_alphanumeric() || "_ -".contains(c)),
        "新文件名只能包含文字、数字、空格、横线和下划线"
    );
    ensure!(catalog_files(p).contains_key(old), "该文件不在素材引用中");
    let source = safe_asset(base, old)?;
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    let relative = format!("assets/{}.{}", stem.trim(), ext);
    let target = base.join(&relative);
    std::fs::create_dir_all(base.join("assets"))?;
    if relative != old {
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut input = std::fs::File::open(source)?;
        std::io::copy(&mut input, &mut out)?;
    }
    let mut draft = p.clone();
    replace_path(&mut draft, old, &relative);
    Ok(draft)
}
fn rename_stage(stage: &mut StageState, kind: AssetKind, old: &str, new: &str) {
    if kind == AssetKind::Character {
        for a in &mut stage.actors {
            if a.character == old {
                a.character = new.into();
            }
        }
    }
    if kind == AssetKind::Background && stage.background.as_deref() == Some(old) {
        stage.background = Some(new.into());
    }
    if kind == AssetKind::Audio {
        for t in [&mut stage.music, &mut stage.ambience]
            .into_iter()
            .flatten()
        {
            if t.audio == old {
                t.audio = new.into();
            }
        }
    }
}
pub fn rename_id(p: &Project, kind: AssetKind, old: &str, new: &str) -> Result<Project> {
    ensure!(
        !new.trim().is_empty() && new.len() <= 120 && new != "$stop",
        "标识不能为空、过长或使用保留值"
    );
    let mut draft = p.clone();
    if old != new {
        let reserved = format!("{kind:?}:{new}");
        if draft.production.asset_aliases.contains_key(&reserved) {
            ensure!(
                resolve_id(p, kind, new) == old,
                "该标识保留给旧存档迁移，不能重复使用"
            );
        }
    }
    match kind {
        AssetKind::Character => {
            ensure!(
                old == new || draft.characters.iter().all(|c| c.id != new),
                "人物标识已存在"
            );
            draft
                .characters
                .iter_mut()
                .find(|c| c.id == old)
                .ok_or_else(|| anyhow::anyhow!("人物不存在"))?
                .id = new.into();
        }
        AssetKind::Background => {
            ensure!(
                old == new || draft.backgrounds.iter().all(|b| b.id != new),
                "背景标识已存在"
            );
            draft
                .backgrounds
                .iter_mut()
                .find(|b| b.id == old)
                .ok_or_else(|| anyhow::anyhow!("背景不存在"))?
                .id = new.into();
        }
        AssetKind::Audio => {
            ensure!(
                old == new || draft.audio.iter().all(|a| a.id != new),
                "音频标识已存在"
            );
            draft
                .audio
                .iter_mut()
                .find(|a| a.id == old)
                .ok_or_else(|| anyhow::anyhow!("音频不存在"))?
                .id = new.into();
        }
        AssetKind::Font => anyhow::bail!("字体没有独立标识"),
    }
    let update = |value: &mut Option<String>| {
        if value.as_deref() == Some(old) {
            *value = Some(new.into());
        }
    };
    for n in &mut draft.nodes {
        match kind {
            AssetKind::Character => {
                update(&mut n.character);
                for a in &mut n.actors {
                    if a.character == old {
                        a.character = new.into();
                    }
                }
            }
            AssetKind::Background => update(&mut n.background),
            AssetKind::Audio => {
                update(&mut n.bgm);
                update(&mut n.sound);
                update(&mut n.direction.voice);
            }
            _ => {}
        }
        for a in &mut n.direction.actions {
            match a {
                StageAction::Show { character, .. }
                | StageAction::Hide { character, .. }
                | StageAction::Move { character, .. }
                | StageAction::Expression { character, .. }
                    if kind == AssetKind::Character && character == old =>
                {
                    *character = new.into()
                }
                StageAction::Background { background, .. }
                    if kind == AssetKind::Background && background == old =>
                {
                    *background = new.into()
                }
                _ => {}
            }
        }
        if kind == AssetKind::Audio {
            for cue in &mut n.direction.audio {
                update(&mut cue.audio);
            }
        }
    }
    if kind == AssetKind::Audio {
        update(&mut draft.presentation.bgm);
    }
    for case in &mut draft.production.cases {
        for step in std::iter::once(&mut case.initial.current).chain(&mut case.initial.history) {
            for stage in [&mut step.stage, &mut step.scene_start]
                .into_iter()
                .flatten()
            {
                rename_stage(stage, kind, old, new);
            }
        }
    }
    if old != new {
        let prefix = format!("{kind:?}:");
        for (key, value) in &mut draft.production.asset_aliases {
            if key.starts_with(&prefix) && value == old {
                *value = new.into();
            }
        }
        draft
            .production
            .asset_aliases
            .remove(&format!("{prefix}{new}"));
        draft
            .production
            .asset_aliases
            .insert(format!("{prefix}{old}"), new.into());
        draft.format_version = 3;
    }
    Ok(draft)
}
pub fn resolve_id(p: &Project, kind: AssetKind, id: &str) -> String {
    let mut id = id.to_owned();
    let mut visited = BTreeSet::new();
    while visited.insert(id.clone()) && visited.len() < 64 {
        let key = format!("{kind:?}:{id}");
        if let Some(next) = p.production.asset_aliases.get(&key) {
            id = next.clone();
        } else {
            break;
        }
    }
    id
}
pub fn migrate_stage(p: &Project, stage: &mut StageState) {
    for actor in &mut stage.actors {
        actor.character = resolve_id(p, AssetKind::Character, &actor.character);
    }
    if let Some(bg) = &mut stage.background {
        *bg = resolve_id(p, AssetKind::Background, bg);
    }
    for track in [&mut stage.music, &mut stage.ambience]
        .into_iter()
        .flatten()
    {
        track.audio = resolve_id(p, AssetKind::Audio, &track.audio);
    }
}
#[derive(Clone, Debug)]
pub struct RepairPlan {
    pub fingerprint: String,
    pub replacements: Vec<(String, AssetKind, PathBuf)>,
    pub unresolved: Vec<String>,
}
pub fn preview_repair(p: &Project, base: &Path, directory: &Path) -> Result<RepairPlan> {
    let candidates = walk(directory)?;
    let mut plan = RepairPlan {
        fingerprint: crate::runtime::fingerprint(p),
        replacements: vec![],
        unresolved: vec![],
    };
    for (file, (kind, _)) in catalog_files(p) {
        if safe_asset(base, &file).is_ok() {
            continue;
        }
        let name = Path::new(&file).file_name();
        let matches: Vec<_> = candidates
            .iter()
            .filter(|p| p.file_name() == name)
            .collect();
        if matches.len() == 1 {
            plan.replacements.push((file, kind, matches[0].clone()));
        } else {
            plan.unresolved.push(file);
        }
    }
    Ok(plan)
}
pub fn apply_repair(p: &Project, base: &Path, plan: &RepairPlan) -> Result<Project> {
    ensure!(
        plan.fingerprint == crate::runtime::fingerprint(p),
        "工程已变化，请重新预览修复"
    );
    let mut draft = p.clone();
    for (old, kind, source) in &plan.replacements {
        let new = import_by_kind(*kind, base, source)?;
        replace_path(&mut draft, old, &new);
    }
    Ok(draft)
}
pub fn voice_matches(p: &Project, directory: &Path) -> Result<(VoiceMatches, Vec<String>)> {
    let files = walk(directory)?;
    let mut matched = vec![];
    let mut ambiguous = vec![];
    let mut stems = BTreeMap::<String, Vec<PathBuf>>::new();
    for f in files {
        let ext = f
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ["wav", "mp3", "ogg", "flac"].contains(&ext.as_str()) {
            stems
                .entry(f.file_stem().unwrap_or_default().to_string_lossy().into())
                .or_default()
                .push(f);
        }
    }
    for n in &p.nodes {
        if let Some(files) = stems.get(&n.id) {
            if files.len() == 1 {
                matched.push((n.id.clone(), files[0].clone()));
            } else {
                ambiguous.push(n.id.clone());
            }
        }
    }
    Ok((matched, ambiguous))
}
pub fn apply_voices(p: &Project, base: &Path, matches: &[(String, PathBuf)]) -> Result<Project> {
    let mut draft = p.clone();
    let mut ids: BTreeSet<_> = p.audio.iter().map(|a| a.id.clone()).collect();
    for (id, file) in matches {
        let relative = import_audio(base, file)?;
        let audio = if let Some(a) = draft.audio.iter().find(|a| a.file == relative) {
            a.id.clone()
        } else {
            let key = (1..)
                .map(|i| format!("voice_{i}"))
                .find(|k| !ids.contains(k))
                .unwrap();
            ids.insert(key.clone());
            draft.audio.push(AudioAsset {
                id: key.clone(),
                name: format!("配音 · {id}"),
                file: relative,
            });
            key
        };
        let n = draft
            .nodes
            .iter_mut()
            .find(|n| &n.id == id)
            .ok_or_else(|| anyhow::anyhow!("匹配节点已不存在"))?;
        n.direction.voice = Some(audio);
    }
    draft.format_version = 3;
    Ok(draft)
}
