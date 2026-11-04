# 开发与架构

OpenGalgame 是 Rust 原生桌面应用，编辑器与独立游戏复用剧情运行和舞台绘制实现。

| 模块 | 职责 |
| --- | --- |
| `src/workspace.rs` / `src/catalog.rs` | 多项目工作区、作品目录、待办、异步加载与导出队列 |
| `src/nodes.rs` / `src/compatibility.rs` | 共享节点快照、按标识索引、版本化存档兼容与旧版基线 |
| `src/studio/images.rs` / `src/studio/checkpoints.rs` | 后台图片解码、纹理缓存、恢复副本与写入取消 |
| `src/main.rs` | 原生窗口与命令行入口 |
| `src/model.rs` | 项目格式、节点、素材、图与条件校验 |
| `src/runtime.rs` | 推进、选项、变量、回退、存档与历史 |
| `src/authoring.rs` | 对白导入、查找替换、连接与循环检测 |
| `src/project_io.rs` | 正式保存、备份、素材复制与导出 |
| `src/recovery.rs` / `src/paths.rs` | 恢复副本、最近项目与旧数据目录兼容 |
| `src/studio.rs` / `src/studio/advanced.rs` | 编辑器、共用舞台、素材库与玩家菜单 |
| `src/sound.rs` / `src/native_icon.rs` | 音轨播放与 Windows EXE 资源图标 |

## 项目格式

项目保存为 UTF-8 JSON。`characters` 定义人物与表情，`backgrounds` 定义背景，`audio` 定义音轨，`presentation` 定义默认演出和标题页，`tasks` 保存创作待办，`save_compatibility` 保存节点迁移和旧版本基线，`flags` 保存初始布尔变量；`nodes` 保存对白、选择和结局，`start` 指定起点。

节点类型为 `dialogue`、`choice`、`ending`。选择包含 `text`、`target`，可设置由 `flag` 和 `value` 组成的 `condition` 与 `set_flag`。素材使用项目目录内的相对路径。

真实样例位于 `examples/first-light/` 与 `examples/long-form/`。旧样例标识保持不变以保留存档兼容性；新增字段使用 serde 默认值。

## 命令行

```powershell
target/release/opengalgame.exe --hub
target/release/opengalgame.exe --validate examples/long-form/project.json
target/release/opengalgame.exe --play examples/long-form/project.json
target/release/opengalgame.exe --editor examples/long-form/project.json
target/release/opengalgame.exe --export examples/long-form/project.json exports/MyGame
```

导出目录必须不存在或为空。校验或导出失败时返回非零退出码。需要等待进程结束的脚本应使用 `subprocess.run` 或 PowerShell `Start-Process -Wait`。

## 构建与发布

需要 Rust MSVC、Visual Studio C++ Build Tools、Windows SDK。Python 3.11+ 仅用于打包脚本，运行编辑器和游戏不需要 Python。

```powershell
cargo build --release --locked
python tools/package_windows.py
```

打包器从 Cargo.toml 读取版本，收集当前 Windows 依赖的许可声明，验证样例与导出结果，再生成编辑器、独立游戏、源码 ZIP 和校验摘要。

`.github/workflows/windows.yml` 在 Windows runner 上运行格式、静态检查、测试与构建；`v*` 标签触发打包并创建 GitHub Release。自动化令牌仅用于当前仓库的发布，日常 Git 远端使用 SSH。
