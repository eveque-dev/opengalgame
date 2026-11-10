use crate::model::{Project, Severity, safe_asset, validate};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub fn read_project(file: &Path) -> Result<Project> {
    ensure!(
        fs::metadata(file)?.len() <= 20 * 1024 * 1024,
        "项目文件超过 20 MB"
    );
    let text = fs::read_to_string(file).context("无法读取项目，请检查路径和文件编码")?;
    let mut p: Project =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).context("JSON 格式错误")?;
    ensure!([1, 2, 3].contains(&p.format_version), "项目格式版本不兼容");
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if p.format_version == 1 && value.get("save_compatibility").is_none() {
        let old = p.clone();
        crate::compatibility::register_legacy_baseline(&mut p, &old)?;
    }
    Ok(p)
}
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    static NEXT_WRITE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT_WRITE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = path.with_extension(format!("{}.{}.{serial}.tmp", std::process::id(), stamp()));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path).with_context(|| format!("无法保存 {}", path.display()))?;
    Ok(())
}
pub fn save_project(file: &Path, project: &Project) -> Result<()> {
    ensure!(
        !project.uses_production() || project.format_version >= 3,
        "新增演出功能需要项目格式 3"
    );
    ensure!(
        !project.uses_typed_logic() || project.format_version >= 2,
        "新增变量和复合条件需要项目格式 2"
    );
    let bytes = serde_json::to_vec_pretty(project)?;
    ensure!(
        bytes.len() <= 20 * 1024 * 1024,
        "项目文件超过 20 MB；请减少重复版本基线或拆分工程后再保存"
    );
    if file.is_file() {
        fs::copy(file, file.with_extension("json.bak"))?;
    }
    write_atomic(file, &bytes)
}
pub fn import_asset(base: &Path, source: &Path) -> Result<String> {
    import_image(base, source, ImageImport::Standard)
}

pub fn import_portrait(base: &Path, source: &Path) -> Result<String> {
    import_image(base, source, ImageImport::Portrait)
}

#[derive(PartialEq, Eq)]
enum ImageImport {
    Standard,
    Portrait,
    Existing,
}

fn import_image(base: &Path, source: &Path, mode: ImageImport) -> Result<String> {
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    ensure!(
        ["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()),
        "请选择 PNG、JPG 或 WebP 图片"
    );
    // A generated RGBA PNG can be larger than the original compressed import.
    let limit_mb = if mode == ImageImport::Existing && ext == "png" {
        258
    } else {
        30
    };
    let limit = limit_mb * 1024 * 1024;
    ensure!(
        fs::metadata(source)?.len() <= limit,
        "素材超过 {limit_mb} MB"
    );
    let mut bytes = fs::read(source)?;
    ensure!(bytes.len() as u64 <= limit, "素材超过 {limit_mb} MB");
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions().context("图片无法解码")?;
    ensure!(width <= 8192 && height <= 8192, "图片不能超过 8192 像素");
    let img = image::load_from_memory(&bytes).context("图片无法解码")?;
    let mut ext = ext;
    if mode == ImageImport::Portrait {
        let mut rgba = img.into_rgba8();
        if crate::portrait::remove_matte(&mut rgba) {
            let mut output = std::io::Cursor::new(Vec::new());
            rgba.write_to(&mut output, image::ImageFormat::Png)?;
            bytes = output.into_inner();
            ext = "png".into();
        }
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let relative = format!("assets/{}.{ext}", &hash[..20]);
    fs::create_dir_all(base.join("assets"))?;
    let target = base.join(&relative);
    if !target.exists() {
        write_atomic(&target, &bytes)?;
    }
    Ok(relative)
}
pub fn clone_project(project: &Project, source_base: &Path, target_file: &Path) -> Result<Project> {
    let target_base = target_file.parent().context("目标目录无效")?;
    fs::create_dir_all(target_base)?;
    let mut copy = project.clone();
    let mut mapped = BTreeMap::new();
    let files: Vec<_> = project
        .image_files()
        .into_iter()
        .map(str::to_owned)
        .chain(project.audio.iter().map(|a| a.file.clone()))
        .chain(
            project
                .production
                .theme
                .iter()
                .filter_map(|t| t.font_file.clone()),
        )
        .collect();
    for relative in files {
        if !mapped.contains_key(&relative) {
            mapped.insert(
                relative.clone(),
                if project
                    .production
                    .theme
                    .as_ref()
                    .and_then(|t| t.font_file.as_ref())
                    == Some(&relative)
                {
                    crate::assets::import_font(target_base, &safe_asset(source_base, &relative)?)?
                } else if project.audio.iter().any(|a| a.file == relative) {
                    import_audio(target_base, &safe_asset(source_base, &relative)?)?
                } else {
                    import_image(
                        target_base,
                        &safe_asset(source_base, &relative)?,
                        ImageImport::Existing,
                    )?
                },
            );
        }
    }
    for b in &mut copy.backgrounds {
        b.file = mapped[&b.file].clone();
    }
    for c in &mut copy.characters {
        if let Some(file) = &c.portrait {
            c.portrait = Some(mapped[file].clone());
        }
        for file in c.expressions.values_mut() {
            *file = mapped[file].clone();
        }
    }
    for a in &mut copy.audio {
        a.file = mapped[&a.file].clone();
    }
    if let Some(theme) = &mut copy.production.theme
        && let Some(file) = &mut theme.font_file
    {
        *file = mapped[file].clone();
    }
    for file in [&mut copy.presentation.cover, &mut copy.presentation.icon]
        .into_iter()
        .flatten()
    {
        *file = mapped[file].clone();
    }
    save_project(target_file, &copy)?;
    Ok(copy)
}
pub fn export_game(
    project: &Project,
    base: &Path,
    executable: &Path,
    destination: &Path,
) -> Result<PathBuf> {
    stage_game(project, base, executable, destination)?;
    fs::copy(executable, destination.join("Game.exe"))?;
    if let Some(icon) = &project.presentation.icon {
        crate::native_icon::set_exe_icon(&destination.join("Game.exe"), &safe_asset(base, icon)?)?;
    }
    fs::write(
        destination.join("游戏说明.txt"),
        format!(
            "{}\r\n\r\n双击 Game.exe 开始游戏。请完整保留 assets 文件夹和 game.json。\r\n点击或空格继续；数字键选择；Esc 打开存档。下方按钮可切换全屏、调整阅读和声音。\r\n新存档位于用户本机 LOCALAPPDATA/opengalgame/saves；旧版数据继续沿用原位置，不写入游戏目录。\r\n本游戏由 OpenGalgame Rust 引擎生成，无需安装 Rust 或 Python。\r\n",
            project.title
        ),
    )?;
    Ok(destination.to_owned())
}

pub(crate) fn stage_game(
    project: &Project,
    base: &Path,
    executable: &Path,
    destination: &Path,
) -> Result<()> {
    let errors: Vec<_> = validate(project, Some(base))
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    ensure!(
        errors.is_empty(),
        "请先修复项目错误：{}",
        errors
            .iter()
            .map(|i| i.message.as_str())
            .collect::<Vec<_>>()
            .join("；")
    );
    ensure!(
        !destination.exists() || fs::read_dir(destination)?.next().is_none(),
        "导出目录不是空目录，请选择新目录，避免覆盖文件"
    );
    fs::create_dir_all(destination)?;
    let mut game = project.clone();
    game.clear_author_notes();
    clone_project(&game, base, &destination.join("game.json"))?;
    fs::write(destination.join("LICENSE"), include_str!("../LICENSE"))?;
    fs::write(
        destination.join("THIRD_PARTY.md"),
        include_str!("../THIRD_PARTY.md"),
    )?;
    if let Some(parent) = executable.parent() {
        let licenses = parent.join("licenses");
        if licenses.is_dir() {
            copy_licenses(&licenses, &destination.join("licenses"))?;
        }
    }
    Ok(())
}

pub fn import_audio(base: &Path, source: &Path) -> Result<String> {
    ensure!(
        fs::metadata(source)?.len() <= 100 * 1024 * 1024,
        "音频不能超过 100 MB"
    );
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    ensure!(
        ["wav", "mp3", "ogg", "flac"].contains(&ext.as_str()),
        "请选择 WAV、MP3、OGG 或 FLAC"
    );
    rodio::Decoder::try_from(fs::File::open(source)?).context("音频无法解码")?;
    let bytes = fs::read(source)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let file = format!("assets/{}.{ext}", &hash[..20]);
    fs::create_dir_all(base.join("assets"))?;
    if !base.join(&file).exists() {
        fs::write(base.join(&file), bytes)?;
    }
    Ok(file)
}
pub fn save_directory(project_id: &str) -> PathBuf {
    let hash = format!("{:x}", Sha256::digest(project_id.as_bytes()));
    crate::paths::data_directory(Path::new("saves").join(&hash[..20]))
}

fn copy_licenses(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "许可目录不能包含符号链接");
        if kind.is_dir() {
            copy_licenses(&entry.path(), &target.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}
