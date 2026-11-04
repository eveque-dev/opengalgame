#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod studio;
use anyhow::{Context, Result, ensure};
use opengalgame::{
    model::{Severity, safe_asset, validate},
    project_io::*,
};
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        if !std::env::args().any(|a| a.starts_with("--")) {
            rfd::MessageDialog::new()
                .set_title("OpenGalgame启动失败")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe()?;
    let directory = exe.parent().context("无法确定程序位置")?;
    let bundled = directory.join("game.json");
    if args.first().map(String::as_str) == Some("--validate") {
        let path = PathBuf::from(
            args.get(1)
                .context("用法：opengalgame.exe --validate project.json")?,
        );
        let p = read_project(&path)?;
        let issues = validate(&p, path.parent());
        for i in &issues {
            println!(
                "{:?} [{}] {}",
                i.severity,
                i.node.as_deref().unwrap_or("project"),
                i.message
            );
        }
        ensure!(
            !issues.iter().any(|i| i.severity == Severity::Error),
            "项目检查失败"
        );
        println!(
            "PASS: {} nodes, {} endings",
            p.nodes.len(),
            p.nodes
                .iter()
                .filter(|n| n.kind == opengalgame::model::NodeKind::Ending)
                .count()
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--export") {
        let path = PathBuf::from(args.get(1).context("--export 需要项目路径和新输出目录")?);
        let dest = PathBuf::from(args.get(2).context("--export 需要输出目录")?);
        let p = read_project(&path)?;
        export_game(&p, path.parent().unwrap_or(Path::new(".")), &exe, &dest)?;
        println!("Exported {}", dest.display());
        return Ok(());
    }
    let mut player = false;
    let path = if args.first().map(String::as_str) == Some("--play") {
        player = true;
        PathBuf::from(args.get(1).context("--play 需要项目路径")?)
    } else if args.first().map(String::as_str) == Some("--editor") {
        PathBuf::from(args.get(1).context("--editor 需要项目路径")?)
    } else if bundled.is_file() {
        player = true;
        bundled
    } else if let Some(file) = args.first() {
        PathBuf::from(file)
    } else {
        let path = directory.join("examples/long-form/project.json");
        if path.is_file() {
            path
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/long-form/project.json")
        }
    };
    let path = path
        .canonicalize()
        .with_context(|| format!("找不到项目：{}", path.display()))?;
    let project = read_project(&path)?;
    if player {
        let issues = validate(&project, path.parent());
        ensure!(
            !issues.iter().any(|i| i.severity == Severity::Error),
            "游戏项目包含错误：{}",
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("；")
        );
    }
    let title = if player {
        format!("{} · OpenGalgame", project.title)
    } else {
        "OpenGalgame".into()
    };
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(if player { [1280., 860.] } else { [1480., 940.] })
        .with_min_inner_size(if player { [820., 620.] } else { [1100., 720.] });
    if let Some(icon) = &project.presentation.icon
        && let Ok(path) = safe_asset(path.parent().unwrap_or(Path::new(".")), icon)
        && let Ok(image) = image::open(path)
    {
        let image = image
            .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        viewport = viewport.with_icon(std::sync::Arc::new(eframe::egui::IconData {
            rgba: image.into_raw(),
            width: 64,
            height: 64,
        }));
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| Ok(Box::new(studio::Studio::new(cc, project, path, player)))),
    )
    .map_err(|e| anyhow::anyhow!("无法建立原生窗口或图形上下文：{e}"))?;
    Ok(())
}
