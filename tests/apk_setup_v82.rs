#![cfg(windows)]

use std::{fs, path::Path, process::Command};

fn isolated_editor(work: &Path) -> Command {
    let runtime = work.join("android-runtime");
    for abi in ["arm64-v8a", "x86_64"] {
        fs::create_dir_all(runtime.join(abi)).unwrap();
        fs::write(runtime.join(abi).join("libopengalgame.so"), b"test").unwrap();
    }
    fs::write(runtime.join("version.txt"), env!("CARGO_PKG_VERSION")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_opengalgame"));
    for name in [
        "JAVA_HOME",
        "JDK_HOME",
        "STUDIO_JDK",
        "ANDROID_HOME",
        "ANDROID_SDK_ROOT",
    ] {
        command.env_remove(name);
    }
    command
        .env("LOCALAPPDATA", work.join("local"))
        .env("ProgramFiles", work.join("Program Files"))
        .env("ProgramW6432", work.join("Program Files"))
        .env("OPENGALGAME_ANDROID_RUNTIME", runtime)
        .env("PATH", "")
        .env("HTTPS_PROXY", "http://127.0.0.1:1");
    command
}

#[test]
fn detects_standard_installs_and_reuses_managed_components_without_network() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path();
    let jdk = root.join("Program Files/BellSoft/jdk-17");
    let sdk = root.join("local/Android/Sdk");
    let managed = root.join("local/opengalgame/apk-toolchain/platform-35/installed");
    for (base, files) in [
        (jdk, &["bin/java.exe", "bin/keytool.exe"][..]),
        (
            sdk.join("build-tools/35.0.0"),
            &["aapt2.exe", "zipalign.exe", "lib/apksigner.jar"][..],
        ),
        (managed, &["android.jar", ".complete"][..]),
    ] {
        for file in files {
            let target = base.join(file);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"test").unwrap();
        }
    }
    fs::write(
        root.join("Program Files/BellSoft/jdk-17/release"),
        "JAVA_VERSION=\"17.0.21\"\n",
    )
    .unwrap();
    let output = isolated_editor(root).arg("--apk-setup").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("BellSoft") && stdout.contains("35.0.0") && stdout.contains("installed")
    );
    assert!(!stdout.contains("下载"));
}

#[test]
fn reports_missing_environment_without_installing_before_consent() {
    let work = tempfile::tempdir().unwrap();
    let check = isolated_editor(work.path())
        .arg("--apk-check")
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert_eq!(
        String::from_utf8(check.stdout)
            .unwrap()
            .matches("待安装")
            .count(),
        3
    );

    let denied = isolated_editor(work.path())
        .arg("--apk-setup")
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(
        String::from_utf8(denied.stderr)
            .unwrap()
            .contains("Android SDK 许可条款")
    );
    assert!(!work.path().join("local/opengalgame/apk-toolchain").exists());
}
