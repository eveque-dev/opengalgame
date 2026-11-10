<p align="center">
  <img src="docs/media/hero.svg" alt="OpenGalgame 视觉小说创作工具" width="100%">
</p>

<p align="center">
  <a href="https://github.com/eveque-dev/opengalgame/releases/latest"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/eveque-dev/opengalgame?style=flat-square&label=release"></a>
  <a href="https://github.com/eveque-dev/opengalgame/actions/workflows/windows.yml"><img alt="Windows build" src="https://github.com/eveque-dev/opengalgame/actions/workflows/windows.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-c5ccd9?style=flat-square"></a>
</p>

# OpenGalgame

OpenGalgame 是一款面向视觉小说创作者的 Windows 可视化编辑器。用人物、对白和选择搭建故事，在同一工作台编排演出、试玩各条路线，并发布为 Windows 或 Android 游戏。编辑器与独立播放器共用 Rust 剧情内核。

[下载 v0.8.2 完整 Windows 便携包](https://github.com/eveque-dev/opengalgame/releases/download/v0.8.2/opengalgame-v0.8.2-windows-x64.zip) · [发布说明](https://github.com/eveque-dev/opengalgame/releases/tag/v0.8.2) · [中文指南](docs/guide.zh-CN.md) · [引擎架构](docs/architecture.md)

## 从样例开始

下载 Windows 编辑器便携 ZIP，解压后双击 `opengalgame.exe`。在项目中心新建双结局模板，或打开 `examples/tidal-stage/project.json`。这个《潮汐剧场》样例包含四个结局、十三段中文配音和可重跑的路线用例。

1. **准备舞台。** 导入背景、立绘和表情。导入人物图片时会识别符合条件的纯色底，并将边缘背景转为透明，保留原画布和源文件。
2. **写对白和选择。** 在剧本表格连续编辑，用分支图连接节点。整数、布尔和文本变量可控制选项出现条件，并记录玩家选择。
3. **试跑并交付。** 按 F5 试玩，检查缺失素材与断链。把关键路线保存成回归用例，再从「导出游戏」选择格式。

<img src="docs/media/graph.png" alt="分支图显示节点、跳转和已试玩路线" width="100%">

## 发布格式

| 格式 | 发给玩家的文件 | 玩家如何打开 |
| --- | --- | --- |
| Windows 单文件 EXE | 一个 `.exe`，内含剧本和素材 | 双击运行 |
| Windows ZIP | 一个 `.zip`，含 `Game.exe`、`game.json` 和素材 | 解压后双击 `Game.exe` |
| Android APK | 一个已签名的 `.apk`，含 ARM64 与 x86_64 播放器 | 在 Android 8.0 及以上的 64 位设备上安装 |

Windows 编辑器与播放器面向 Windows 10/11 x64，需要可用的 OpenGL 3.3 驱动。

### 一键准备 Android 导出

使用完整 Windows 便携包，打开项目后选择「导出游戏 → Android APK」。编辑器会识别已有的 JDK 17 或更新版本、Android SDK Platform 35 和 Build-Tools 35 或更新版本。环境齐全时可直接选择保存位置。

缺少组件时，窗口会列出待安装项及预计下载大小。首次安装 SDK 组件需要确认许可。点击「安装并继续导出」后，编辑器会校验下载并将组件安装到当前用户目录，完成后自动弹出保存窗口。之后的项目会复用这套工具。 [安装、签名和命令行用法](docs/distribution.md)

## 创作工作台

| 工作 | 可以做什么 |
| --- | --- |
| 剧本与逻辑 | 批量导入对白、CSV 往返、复合条件、按章组织、全文查找 |
| 画面与声音 | 多人同屏、表情切换、移动和淡入淡出、逐句配音、独立环境音 |
| 调试与存档 | 断点、隐藏选项解释、路线复现、结局覆盖、按节点迁移存档 |
| 多项目管理 | 独立撤销与试玩进度、跨项目待办、自动恢复副本、后台导出 |
| AI 创作 | 自填 API 密钥、附选人物卡与前情、生成可编辑分支、分批检查全剧 |

AI 功能支持 DeepSeek、OpenAI、硅基流动、百炼和兼容接口。发送前可预览内容，导出游戏会剔除人物设定卡与前情摘要。编辑、试玩和玩家游玩可离线运行。[AI 创作与配色](docs/ai-assistant.md) · [演出与制作](docs/production-v5.md) · [存档兼容](docs/save-compatibility.md)

## 从源码运行

在 Windows 上安装 Rust MSVC、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
git clone git@github.com:eveque-dev/opengalgame.git
cd opengalgame
cargo run --locked
cargo test --locked
```

作品以 UTF-8 JSON 保存，编辑界面使用 egui/eframe。剧情状态、存档、舞台渲染和媒体播放共用于 Windows 与 Android 播放器。源码包也包含样例、测试和发布脚本。

[贡献指南](CONTRIBUTING.md) · [MIT 许可证](LICENSE) · [第三方组件及示例素材](THIRD_PARTY.md)
