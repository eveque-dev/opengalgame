# 第三方组件

本软件采用 Rust crates.io 的组件，精确版本记录于 Cargo.lock。

- egui / eframe 0.33.3：MIT OR Apache-2.0，https://github.com/emilk/egui
- ab_glyph：Apache-2.0，https://github.com/alexheretic/ab-glyph
- image：MIT OR Apache-2.0，https://github.com/image-rs/image
- rfd 0.17.2：MIT，https://github.com/PolyMeilex/rfd
- serde / serde_json：MIT OR Apache-2.0，https://github.com/serde-rs
- anyhow：MIT OR Apache-2.0，https://github.com/dtolnay/anyhow
- sha2：MIT OR Apache-2.0，https://github.com/RustCrypto/hashes
- reqwest：MIT OR Apache-2.0，https://github.com/seanmonstar/reqwest
- rodio 0.21.1：MIT OR Apache-2.0，https://github.com/RustAudio/rodio
- cpal：Apache-2.0，https://github.com/RustAudio/cpal
- Symphonia 音频解码：MPL-2.0，https://github.com/pdeljanov/Symphonia
- windows-sys：MIT OR Apache-2.0，https://github.com/microsoft/windows-rs

目标平台的依赖元数据与注册表中可用的许可文件由打包脚本收集到发布包 `licenses/`。版本、许可声明、源仓库及所收集文件见 `licenses/index.json`。MPL 组件未修改，源代码可从索引中的上游仓库或 crates.io 对应版本获取。

程序运行时读取 Windows 系统中文字体，不随包分发该字体。egui 内置字体的原始许可随其依赖许可一并保留。示例美术来自本工作区之前生成的图像，用于演示，可以替换为自己的授权素材。

《潮汐剧场》的演示配音由本机 Windows System.Speech 调用已安装中文语音生成，随包提供 WAV 输出，不附带或再分发语音引擎。环境音、和弦和提示音由示例脚本合成。
