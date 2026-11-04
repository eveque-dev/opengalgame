# 参与 OpenGalgame

感谢帮助 OpenGalgame 成为更顺手的视觉小说创作工具。品牌名称为 **OpenGalgame**；仓库、Rust crate、命令与可执行文件使用小写 **opengalgame**。

## 报告问题

请描述操作步骤、预期与实际结果，并提供 Windows 版本、程序版本和必要的截图。能用最小 JSON 项目复现的问题更容易定位。分享样例前请移除私人内容与无权公开的素材。

## 提交修改

1. Fork 仓库，为一个明确的问题创建分支。
2. 保持 UTF-8 编码；Windows `.cmd` 文件使用 CRLF。
3. 修改项目格式时，保留旧字段的默认值，说明对项目和存档兼容性的影响。
4. 修改运行行为时，添加能验证实际行为的测试。
5. 提交 PR，说明问题、改变后的行为和验证结果。界面改动请附实际截图。

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

目前主要验证 Windows x64。请避免把本机绝对路径、凭据、构建缓存或临时验收项目加入提交。
