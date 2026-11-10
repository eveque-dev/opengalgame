use super::{ABIS, TARGET_SDK, Tools};
use crate::paths::data_directory;
use anyhow::{Context, Result, ensure};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const INSTALLED: &str = ".complete";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dependency {
    Jdk,
    Platform,
    BuildTools,
}

impl Dependency {
    pub const ALL: [Self; 3] = [Self::Jdk, Self::Platform, Self::BuildTools];

    pub fn label(self) -> &'static str {
        match self {
            Self::Jdk => "JDK 17 或更新版本",
            Self::Platform => "Android SDK Platform 35",
            Self::BuildTools => "Android Build-Tools 35 或更新版本",
        }
    }

    pub fn download_bytes(self) -> u64 {
        self.archive().size
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Jdk => "jdk",
            Self::Platform => "platform-35",
            Self::BuildTools => "build-tools",
        }
    }

    fn usable(self, directory: &Path) -> bool {
        match self {
            Self::Jdk => {
                ["bin/java.exe", "bin/keytool.exe"]
                    .into_iter()
                    .all(|file| directory.join(file).is_file())
                    && fs::read_to_string(directory.join("release"))
                        .ok()
                        .and_then(|release| {
                            release
                                .lines()
                                .find_map(|line| line.strip_prefix("JAVA_VERSION=\""))
                                .and_then(|value| {
                                    value
                                        .split(['.', '-', '+', '"'])
                                        .next()?
                                        .parse::<u32>()
                                        .ok()
                                })
                        })
                        .is_some_and(|major| major >= 17)
            }
            Self::Platform => directory.join("android.jar").is_file(),
            Self::BuildTools => ["aapt2.exe", "zipalign.exe", "lib/apksigner.jar"]
                .into_iter()
                .all(|file| directory.join(file).is_file()),
        }
    }

    fn archive(self) -> Archive {
        match self {
            // Published Temurin SHA-256 and Google repository archives verified against repository2-1.xml.
            Self::Jdk => Archive {
                url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.21%2B9/OpenJDK17U-jdk_x64_windows_hotspot_17.0.21_9.zip",
                sha256: "98518ecf554465057ee5509462631deeee374bc368c12a411d225cd29381f84b",
                prefix: "jdk-17.0.21+9",
                size: 191_117_380,
            },
            Self::Platform => Archive {
                url: "https://dl.google.com/android/repository/platform-35_r02.zip?opengalgame=1",
                sha256: "0988cacad01b38a18a47bac14a0695f246bc76c1b06c0eeb8eb0dc825ab0c8e0",
                prefix: "android-35",
                size: 64_273_788,
            },
            Self::BuildTools => Archive {
                url: "https://dl.google.com/android/repository/build-tools_r35_windows.zip?opengalgame=1",
                sha256: "5753c679a1b90bcf6fbc9945a2ce39dfb9e74f1df0831a1e1866ae5b594326f0",
                prefix: "android-15",
                size: 59_878_107,
            },
        }
    }
}

struct Archive {
    url: &'static str,
    sha256: &'static str,
    prefix: &'static str,
    size: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum InstallProgress {
    Download {
        dependency: Dependency,
        received: u64,
    },
    Extract(Dependency),
}

pub struct Setup {
    jdk: Option<PathBuf>,
    platform: Option<PathBuf>,
    build_tools: Option<PathBuf>,
    runtime: PathBuf,
}

fn child_directories(path: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
}

fn environment_paths<const N: usize>(variables: [&str; N]) -> impl Iterator<Item = PathBuf> {
    variables
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn jdk(path: &[PathBuf]) -> Option<PathBuf> {
    let program_files = environment_paths(["ProgramW6432", "ProgramFiles"]);
    let standard = program_files.flat_map(|root| {
        let mut homes = vec![root.join("Android/Android Studio/jbr")];
        for vendor in [
            "Java",
            "Eclipse Adoptium",
            "Microsoft",
            "BellSoft",
            "Amazon Corretto",
            "Zulu",
        ] {
            homes.extend(child_directories(&root.join(vendor)));
        }
        homes
    });
    environment_paths(["JAVA_HOME", "JDK_HOME", "STUDIO_JDK"])
        .chain(
            path.iter()
                .filter_map(|path| path.parent().map(Path::to_owned)),
        )
        .chain(standard)
        .find(|home| Dependency::Jdk.usable(home))
}

fn sdk_roots(path: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = environment_paths(["ANDROID_HOME", "ANDROID_SDK_ROOT"]).collect::<Vec<_>>();
    roots.extend(environment_paths(["LOCALAPPDATA"]).map(|path| path.join("Android/Sdk")));
    for directory in path {
        roots.extend(
            directory
                .ancestors()
                .take(4)
                .filter(|root| root.join("build-tools").is_dir() || root.join("platforms").is_dir())
                .map(Path::to_owned),
        );
    }
    roots
}

fn build_tools(root: &Path) -> Option<PathBuf> {
    child_directories(&root.join("build-tools"))
        .filter_map(|path| {
            let version = path
                .file_name()?
                .to_str()?
                .split('.')
                .map(str::parse::<u32>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .ok()?;
            (version.first().copied()? >= 35 && Dependency::BuildTools.usable(&path))
                .then_some((version, path))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, path)| path)
}

impl Setup {
    pub fn detect(editor: &Path) -> Result<Self> {
        let runtime = environment_paths(["OPENGALGAME_ANDROID_RUNTIME"])
            .chain(editor.parent().map(|path| path.join("android-runtime")))
            .find(|path| {
                ABIS.iter().all(|abi| path.join(abi).join("libopengalgame.so").is_file())
                    && fs::read_to_string(path.join("version.txt"))
                        .is_ok_and(|version| version.trim() == env!("CARGO_PKG_VERSION"))
            })
            .context("未找到此版本的 Android 播放器。请使用完整编辑器 ZIP，并保留 android-runtime 目录。")?;
        let path = env::var_os("PATH")
            .map(|value| {
                env::split_paths(&value)
                    .filter(|path| path.is_absolute())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let roots = sdk_roots(&path);
        let managed = data_directory("apk-toolchain");
        let installed = |dependency: Dependency| {
            child_directories(&managed.join(dependency.directory()))
                .find(|path| path.join(INSTALLED).is_file() && dependency.usable(path))
        };
        Ok(Self {
            jdk: jdk(&path).or_else(|| installed(Dependency::Jdk)),
            platform: roots
                .iter()
                .map(|root| root.join(format!("platforms/android-{TARGET_SDK}")))
                .find(|path| Dependency::Platform.usable(path))
                .or_else(|| installed(Dependency::Platform)),
            build_tools: roots
                .iter()
                .find_map(|root| build_tools(root))
                .or_else(|| installed(Dependency::BuildTools)),
            runtime,
        })
    }

    pub fn location(&self, dependency: Dependency) -> Option<&Path> {
        match dependency {
            Dependency::Jdk => self.jdk.as_deref(),
            Dependency::Platform => self.platform.as_deref(),
            Dependency::BuildTools => self.build_tools.as_deref(),
        }
    }

    pub fn missing(&self) -> impl Iterator<Item = Dependency> + '_ {
        Dependency::ALL
            .into_iter()
            .filter(|dep| self.location(*dep).is_none())
    }

    pub(super) fn into_tools(self) -> Result<Tools> {
        let missing = self.missing().map(Dependency::label).collect::<Vec<_>>();
        ensure!(
            missing.is_empty(),
            "APK 导出缺少 {}。请在编辑器的「Android APK」中安装。",
            missing.join("、")
        );
        let jdk = self.jdk.unwrap();
        Ok(Tools {
            build: self.build_tools.unwrap(),
            platform: self.platform.unwrap().join("android.jar"),
            java: jdk.join("bin/java.exe"),
            keytool: jdk.join("bin/keytool.exe"),
            runtime: self.runtime,
        })
    }

    pub fn install(
        editor: &Path,
        accepted_sdk: bool,
        cancel: &AtomicBool,
        mut report: impl FnMut(InstallProgress),
    ) -> Result<Self> {
        static INSTALL: Mutex<()> = Mutex::new(());
        let _guard = INSTALL
            .lock()
            .map_err(|_| anyhow::anyhow!("安装任务异常中止，请重新启动编辑器"))?;
        check_cancel(cancel)?;
        let setup = Self::detect(editor)?;
        let missing = setup.missing().collect::<Vec<_>>();
        ensure!(
            accepted_sdk || missing.iter().all(|item| *item == Dependency::Jdk),
            "请先阅读并同意 Android SDK 许可条款"
        );
        if missing.is_empty() {
            return Ok(setup);
        }
        let root = data_directory("apk-toolchain");
        fs::create_dir_all(&root)?;
        let client = Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(45))
            .user_agent(concat!("OpenGalgame/", env!("CARGO_PKG_VERSION")))
            .build()?;
        for dependency in missing {
            install_component(&client, &root, dependency, cancel, &mut report)
                .with_context(|| format!("安装 {} 失败", dependency.label()))?;
        }
        let setup = Self::detect(editor)?;
        ensure!(
            setup.missing().next().is_none(),
            "部分工具未通过安装后的检查，请重试"
        );
        Ok(setup)
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "已取消安装");
    Ok(())
}

fn install_component(
    client: &Client,
    root: &Path,
    dependency: Dependency,
    cancel: &AtomicBool,
    report: &mut impl FnMut(InstallProgress),
) -> Result<()> {
    check_cancel(cancel)?;
    let package = dependency.archive();
    let stage = tempfile::TempDir::new_in(root)?;
    let zip = stage.path().join("download.zip");
    report(InstallProgress::Download {
        dependency,
        received: 0,
    });
    let mut response = client
        .get(package.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(reqwest::Error::without_url)
        .context("无法下载官方软件包")?;
    let mut file = fs::File::create(&zip)?;
    let mut sha = Sha256::new();
    let mut received = 0;
    let mut last_report = Instant::now();
    let mut buffer = vec![0; 128 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = response.read(&mut buffer).context("软件下载中断")?;
        if count == 0 {
            break;
        }
        received += count as u64;
        ensure!(received <= package.size, "下载内容超出软件包大小");
        sha.update(&buffer[..count]);
        file.write_all(&buffer[..count])?;
        if last_report.elapsed() >= Duration::from_millis(150) {
            report(InstallProgress::Download {
                dependency,
                received,
            });
            last_report = Instant::now();
        }
    }
    ensure!(
        received == package.size && format!("{:x}", sha.finalize()) == package.sha256,
        "下载未通过大小和 SHA-256 校验"
    );
    drop(file);
    report(InstallProgress::Extract(dependency));
    let destination = root.join(dependency.directory());
    fs::create_dir_all(&destination)?;
    let pending = tempfile::TempDir::new_in(destination)?;
    let required = (dependency == Dependency::Platform).then_some(Path::new("android.jar"));
    unpack(
        fs::File::open(zip)?,
        pending.path(),
        package.prefix,
        required,
        cancel,
    )?;
    ensure!(dependency.usable(pending.path()), "解包后的组件不完整");
    check_cancel(cancel)?;
    // Detect only completed directories without moving files Windows may still be scanning.
    fs::File::options()
        .write(true)
        .create_new(true)
        .open(pending.path().join(INSTALLED))?;
    let _ = pending.keep();
    Ok(())
}

fn unpack<R: Read + io::Seek>(
    file: R,
    target: &Path,
    prefix: &str,
    required: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(file)?;
    ensure!(zip.len() <= 30_000, "安装包文件过多");
    let mut total = 0_u64;
    for index in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(index)?;
        let path = entry.enclosed_name().context("安装包路径无效")?;
        ensure!(
            !entry.is_symlink()
                && !entry.name().contains(['\\', ':'])
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "安装包包含不支持的文件路径"
        );
        let relative = path.strip_prefix(prefix).context("安装包目录与预期不符")?;
        if required.is_some_and(|file| file != relative) {
            continue;
        }
        let output = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(output)?;
        } else {
            let size = entry.size();
            total = total.saturating_add(size);
            ensure!(
                !relative.as_os_str().is_empty() && total <= 1024 * 1024 * 1024,
                "安装包文件大小异常"
            );
            fs::create_dir_all(output.parent().unwrap())?;
            let mut file = fs::File::options()
                .write(true)
                .create_new(true)
                .open(output)?;
            let actual = io::copy(&mut entry.by_ref().take(size + 1), &mut file)?;
            ensure!(actual == size, "安装包内容不完整");
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn refuses_to_extract_zip_entries_outside_the_component() {
    let mut archive = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    archive
        .start_file(
            "jdk/../escape.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"test").unwrap();
    let work = tempfile::tempdir().unwrap();
    let result = unpack(
        archive.finish().unwrap(),
        &work.path().join("component"),
        "jdk",
        None,
        &AtomicBool::new(false),
    );
    assert!(result.is_err());
    assert!(!work.path().join("escape.txt").exists());
}
