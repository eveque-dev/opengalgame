# OpenGalgame 0.8.2

## APK 导出环境可在编辑器内安装

- 选择「导出游戏 → Android APK」后，自动检测 JDK 17 或更新版本、Android SDK Platform 35 和 Build-Tools 35 或更新版本。环境齐全时直接选择保存位置。
- 缺少组件时，可在查看下载大小及适用的 SDK 许可后选择「安装并继续导出」。下载沿用 Windows 的代理设置，并通过 SHA-256 校验。工具存放在当前用户的 OpenGalgame 数据目录。
- 安装完成后自动打开保存对话框。后续导出复用已安装的工具，支持离线生成已签名的 APK。

## 下载

解压完整 Windows 便携包，双击 `opengalgame.exe`，保留同目录的 `android-runtime/`。发布资产还包括三个游戏样例、源码包和 SHA-256 校验文件。 [完整 Windows 便携包](https://github.com/eveque-dev/opengalgame/releases/download/v0.8.2/opengalgame-v0.8.2-windows-x64.zip)

99 项自动测试通过。隔离 Windows 环境已验证首次下载和安装、离线复用，以及 APK 签名与对齐。 [导出与签名指南](https://github.com/eveque-dev/opengalgame/blob/v0.8.2/docs/distribution.md)
