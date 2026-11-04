# OpenGalgame 0.3.1

本次 0.3 系列交付版，包含项目中心、多项目工作区、跨作品创作待办、后台导出、存档兼容与长篇性能优化。完整功能见 [0.3 功能说明](https://github.com/eveque-dev/opengalgame/blob/main/docs/release-v0.3.0.md)。

修复节点改名后，新建节点或批量导入可能复用旧标识的问题。已记录为存档迁移入口的旧标识，会保留给旧存档，不再分配给新的剧情节点。

40 项自动测试，包含真实 0.2 存档与 5000 节点快照、索引、循环检测验证。实际原生窗口验证多项目切换、待办归属、退出保护、工作区恢复和大剧本后台导出。

## 下载

- **`opengalgame-v0.3.1-windows-x64.zip`**：推荐，便携编辑器，附完整样例、说明与许可，解压双击 `opengalgame.exe`。
- **`opengalgame.exe`**：单文件编辑器，双击进入项目中心，可直接新建故事。
- **`three-letters-v0.3.1-windows-x64.zip`**：42 节点、3 结局的独立示例游戏。
- **`opengalgame-v0.3.1-source.zip`**：源码、测试与打包工具。
- **`SHA256SUMS.txt`**：文件校验值。

目标 Windows 10/11 x64，需要 OpenGL 3.3 图形驱动。新版存档需要 0.3 播放器；控制流程重写仍需作者处理迁移。无需安装 Rust、Python 或 WebView。
