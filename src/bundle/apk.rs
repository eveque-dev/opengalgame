//! Package the shared Rust player with Android's installed build tools.
use super::*;
use crate::{model::Project, paths::data_directory};
use sha2::{Digest, Sha256};
use std::{os::windows::process::CommandExt, process::Command};

mod setup;
pub use setup::{Dependency, InstallProgress, Setup};

const ABIS: [&str; 2] = ["arm64-v8a", "x86_64"];
const MIN_SDK: u32 = 26;
const TARGET_SDK: u32 = 35;

pub struct Tools {
    build: PathBuf,
    platform: PathBuf,
    java: PathBuf,
    keytool: PathBuf,
    runtime: PathBuf,
}

impl Tools {
    pub fn find(editor: &Path) -> Result<Self> {
        Setup::detect(editor)?.into_tools()
    }

    pub(super) fn build(
        &self,
        project: &Project,
        game: &Path,
        stage: &Path,
        output: &Path,
    ) -> Result<()> {
        let assets = stage.join("assets");
        fs::create_dir(&assets)?;
        write_archive(File::create(assets.join("game.zip"))?, game, None)?;
        // Retain dependency notices in the installed APK as well.
        let notices = self.runtime.join("notices.zip");
        if notices.is_file() {
            fs::copy(notices, assets.join("android-notices.zip"))?;
        }
        let manifest = stage.join("AndroidManifest.xml");
        fs::write(&manifest, manifest_xml(project))?;
        let unaligned = stage.join("unaligned.apk");
        checked(
            Command::new(self.build.join("aapt2.exe"))
                .arg("link")
                .arg("-o")
                .arg(&unaligned)
                .arg("-I")
                .arg(&self.platform)
                .arg("--manifest")
                .arg(&manifest)
                .arg("-A")
                .arg(&assets)
                .args(["-0", "zip"]),
            "编译 Android 清单",
        )?;
        let file = File::options().read(true).write(true).open(&unaligned)?;
        let mut apk = ZipWriter::new_append(file)?;
        for abi in ABIS {
            apk.start_file(
                format!("lib/{abi}/libopengalgame.so"),
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )?;
            io::copy(
                &mut File::open(self.runtime.join(abi).join("libopengalgame.so"))?,
                &mut apk,
            )?;
        }
        apk.finish()?;
        let aligned = stage.join("aligned.apk");
        checked(
            Command::new(self.build.join("zipalign.exe"))
                .args(["-P", "16", "-f", "4"])
                .arg(&unaligned)
                .arg(&aligned),
            "对齐 APK",
        )?;
        let key = self.signing_key()?;
        let signer = self.build.join("lib/apksigner.jar");
        checked(
            Command::new(&self.java)
                .arg("-jar")
                .arg(&signer)
                .args(["sign", "--ks"])
                .arg(key)
                .args([
                    "--ks-key-alias",
                    "opengalgame",
                    "--ks-pass",
                    "pass:opengalgame-local",
                    "--out",
                ])
                .arg(output)
                .arg(aligned),
            "签名 APK",
        )?;
        checked(
            Command::new(&self.java)
                .arg("-jar")
                .arg(signer)
                .arg("verify")
                .arg(output),
            "验证 APK 签名",
        )
    }

    fn signing_key(&self) -> Result<PathBuf> {
        let directory = data_directory("android-signing");
        fs::create_dir_all(&directory)?;
        let key = directory.join("player.p12");
        if key.is_file() {
            return Ok(key);
        }
        let stage = TempDir::new_in(directory)?;
        let pending = stage.path().join("player.p12");
        checked(
            Command::new(&self.keytool)
                .args(["-genkeypair", "-keystore"])
                .arg(&pending)
                .args([
                    "-storetype",
                    "PKCS12",
                    "-storepass",
                    "opengalgame-local",
                    "-alias",
                    "opengalgame",
                    "-keyalg",
                    "RSA",
                    "-keysize",
                    "2048",
                    "-validity",
                    "10000",
                    "-dname",
                    "CN=OpenGalgame Local Export",
                    "-noprompt",
                ]),
            "创建本机 Android 签名密钥",
        )?;
        match fs::hard_link(&pending, &key) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e).context("无法保存本机 Android 签名密钥"),
        }
        Ok(key)
    }
}

fn checked(command: &mut Command, action: &str) -> Result<()> {
    let program = command.get_program().to_owned();
    let output = command
        .creation_flags(0x0800_0000)
        .output()
        .with_context(|| format!("{action}失败，无法运行 {}", Path::new(&program).display()))?;
    let details = [output.stdout.as_slice(), output.stderr.as_slice()].concat();
    ensure!(
        output.status.success(),
        "{action}失败：{}",
        String::from_utf8_lossy(&details).trim()
    );
    Ok(())
}

fn manifest_xml(project: &Project) -> String {
    let id = format!("{:x}", Sha256::digest(project.id.as_bytes()));
    let label = project
        .title
        .chars()
        .take(80)
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .replace('\\', r"\\")
        .replace('\'', r"\'")
        .replace('@', r"\@")
        .replace('?', r"\?")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', r"\&quot;");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="io.opengalgame.game.p{id:.24}" android:versionCode="1" android:versionName="1.0">
    <uses-sdk android:minSdkVersion="{MIN_SDK}" android:targetSdkVersion="{TARGET_SDK}" />
    <application android:label="{label}" android:hasCode="false" android:extractNativeLibs="true"
        android:theme="@android:style/Theme.Material.NoActionBar.Fullscreen">
        <activity android:name="android.app.NativeActivity" android:exported="true"
            android:screenOrientation="sensorLandscape"
            android:configChanges="orientation|screenSize|keyboardHidden">
            <meta-data android:name="android.app.lib_name" android:value="opengalgame" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#
    )
}
