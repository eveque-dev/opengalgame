# 发布 Windows 与 Android 游戏

在编辑器顶部点「导出游戏」，选择格式和新的保存位置。导出进入后台队列，可在「后台任务」查看进度。所有格式都会带上项目的剧本、引用素材和许可文件，剔除人物设定卡与前情摘要。

| 格式 | 交付和运行 |
| --- | --- |
| Windows 单文件 EXE | 发送单个 EXE。双击后播放器将内嵌游戏解包到临时目录，存档仍写到用户数据目录。 |
| Windows ZIP | 发送 ZIP。解压后双击 `Game.exe`，保留 `game.json`、`assets/` 和许可文件。 |
| Android APK | 发送已签名 APK。玩家在 Android 8.0 及以上的 ARM64 或 x86_64 设备上安装，横屏触控游玩，存档留在应用数据中。 |

立绘和表情导入时会检测纯色或接近纯色的边缘背景。符合条件时生成透明 PNG，保留画布尺寸和原始文件。已有透明区域的图片会原样保存。复杂背景建议预先处理为透明 PNG。已导入的旧立绘可用「导入 / 替换默认立绘」重新选择原图。

## Android 导出环境

1. 下载并解压完整 Windows 编辑器 ZIP，保留 `opengalgame.exe` 同目录下的 `android-runtime/`。
2. 安装 JDK 17，确保 `JAVA_HOME` 或 PATH 指向该 JDK。安装 Android SDK Platform 35 和 Build-Tools 35.0.0 或更高版本。
3. SDK 如果放在默认 Android Studio 路径之外，在 Windows 环境变量中设置 `ANDROID_HOME` 为 SDK 根目录。打开编辑器，选择「Android APK」。

已安装 Android 命令行工具时，可用以下命令安装所需组件。导出时无需为每部作品重新编译播放器。 [Android SDK 命令行工具](https://developer.android.com/tools/sdkmanager)

```powershell
sdkmanager "platforms;android-35" "build-tools;35.0.0"
```

APK 用于直接安装。编辑器会在第一次导出时生成本机签名密钥，位于 `%LOCALAPPDATA%/opengalgame/android-signing/player.p12`。若使用了旧版数据目录，则沿用对应位置。作品包名由项目标识生成。要让后续 APK 覆盖更新并保留存档，请保留原项目标识和这份密钥，迁移机器时私下备份并恢复它。

## 命令行

```powershell
.\opengalgame.exe --export-exe examples/tidal-stage/project.json exports/潮汐剧场.exe
.\opengalgame.exe --export-zip examples/tidal-stage/project.json exports/潮汐剧场.zip
.\opengalgame.exe --export-apk examples/tidal-stage/project.json exports/潮汐剧场.apk
.\exports\潮汐剧场.exe --verify-bundle
```

`--verify-bundle` 会读取 EXE 内嵌的剧本和素材，执行项目校验及分支测试。旧版脚本仍可使用 `--export 项目.json 输出目录` 生成松散的 Windows 游戏目录。

## 从源码构建 Android 播放器

先安装 Android NDK r28c，并准备两个 Rust Android 目标。打包脚本使用 Python 3.11 或以上。

```powershell
sdkmanager "ndk;28.2.13676358"
rustup target add aarch64-linux-android x86_64-linux-android
python tools/build_android.py
cargo build --release --locked
python tools/package_windows.py
```

手动运行源码构建的编辑器时，可将 `OPENGALGAME_ANDROID_RUNTIME` 设为生成的 `target/android-runtime` 的完整路径。完整编辑器 ZIP 已带上此目录。
