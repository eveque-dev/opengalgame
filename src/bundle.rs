//! The editor and command line use the same packaging path.
use crate::{model::Project, project_io::*};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::{NamedTempFile, TempDir};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[cfg(windows)]
pub mod apk;

const EXE_MARKER: &str = "OpenGalgame/player-bundle/v1";
const MAX_EXPANDED: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Exe,
    Zip,
    Apk,
}

impl Format {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Zip => "zip",
            Self::Apk => "apk",
        }
    }
}

pub fn export(
    project: &Project,
    base: &Path,
    editor: &Path,
    destination: &Path,
    format: Format,
) -> Result<PathBuf> {
    ensure!(!destination.exists(), "目标文件已存在，请使用新文件名");
    ensure!(
        destination
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(format.extension())),
        "请使用 .{} 文件扩展名",
        format.extension()
    );
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = TempDir::new_in(parent)?;
    let game = stage.path().join("game");
    let output = match format {
        Format::Exe => {
            stage_game(project, base, editor, &game)?;
            let exe = stage.path().join("player.exe");
            fs::copy(editor, &exe)?;
            if let Some(icon) = &project.presentation.icon {
                crate::native_icon::set_exe_icon(&exe, &crate::model::safe_asset(base, icon)?)?;
            }
            let mut file = OpenOptions::new().write(true).open(&exe)?;
            file.seek(SeekFrom::End(0))?;
            write_archive(file, &game, Some(EXE_MARKER))?;
            exe
        }
        Format::Zip => {
            export_game(project, base, editor, &game)?;
            let zip = stage.path().join("game.zip");
            write_archive(File::create(&zip)?, &game, None)?;
            zip
        }
        Format::Apk => {
            #[cfg(windows)]
            {
                let tools = apk::Tools::find(editor)?;
                stage_game(project, base, editor, &game)?;
                let output = stage.path().join("game.apk");
                tools.build(project, &game, stage.path(), &output)?;
                output
            }
            #[cfg(not(windows))]
            anyhow::bail!("APK 导出请使用 Windows 编辑器");
        }
    };
    // Publish a complete file without overwriting a concurrent export.
    let (temp, path) = NamedTempFile::new_in(parent)?.into_parts();
    let mut temp = temp;
    io::copy(&mut File::open(output)?, &mut temp)?;
    temp.sync_all()?;
    drop(temp);
    path.persist_noclobber(destination)?;
    Ok(destination.to_owned())
}

fn write_archive<W: Write + Seek>(writer: W, root: &Path, comment: Option<&str>) -> Result<()> {
    fn add<W: Write + Seek>(zip: &mut ZipWriter<W>, root: &Path, directory: &Path) -> Result<()> {
        let mut entries = fs::read_dir(directory)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                add(zip, root, &path)?;
            } else {
                ensure!(kind.is_file(), "打包目录包含不支持的文件");
                let name = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let mut file = File::open(&path)?;
                let options = SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .large_file(file.metadata()?.len() >= u64::from(u32::MAX));
                zip.start_file(name, options)?;
                io::copy(&mut file, zip)?;
            }
        }
        Ok(())
    }

    let mut zip = ZipWriter::new(writer);
    if let Some(comment) = comment {
        zip.set_comment(comment);
    }
    add(&mut zip, root, root)?;
    zip.finish()?;
    Ok(())
}

pub fn open_exe(executable: &Path) -> Result<Option<TempDir>> {
    let mut file = File::open(executable)?;
    let mut marker = [0; EXE_MARKER.len()];
    if file.metadata()?.len() < marker.len() as u64 {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-(marker.len() as i64)))?;
    file.read_exact(&mut marker)?;
    if marker != EXE_MARKER.as_bytes() {
        return Ok(None);
    }
    let directory = TempDir::with_prefix("opengalgame-player-")?;
    extract(file, directory.path()).context("无法解开 EXE 内嵌游戏")?;
    Ok(Some(directory))
}

#[cfg(any(windows, target_os = "android"))]
pub(crate) fn extract<R: Read + Seek>(reader: R, directory: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(reader)?;
    ensure!(archive.len() <= 30_000, "游戏包文件过多");
    let mut total = 0_u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.enclosed_name().context("游戏包路径无效")?;
        ensure!(
            !entry.is_symlink()
                && !entry.name().contains(['\\', ':'])
                && name
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "游戏包包含不支持的文件路径"
        );
        let output = directory.join(name);
        if entry.is_dir() {
            fs::create_dir_all(output)?;
        } else {
            total = total.saturating_add(entry.size());
            ensure!(total <= MAX_EXPANDED, "游戏包解压后超过 8 GiB");
            fs::create_dir_all(output.parent().unwrap())?;
            let mut file = File::options().create_new(true).write(true).open(output)?;
            let size = entry.size();
            let bytes = io::copy(&mut (&mut entry).take(size + 1), &mut file)?;
            ensure!(bytes == size, "游戏包文件大小不匹配");
        }
    }
    ensure!(
        directory.join("game.json").is_file(),
        "游戏包缺少 game.json"
    );
    Ok(())
}
