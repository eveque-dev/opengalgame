#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod studio;
mod workspace;
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
    let initial = match args.first().map(String::as_str) {
        Some("--play") => {
            player = true;
            Some(PathBuf::from(args.get(1).context("--play 需要项目路径")?))
        }
        Some("--editor") => Some(PathBuf::from(args.get(1).context("--editor 需要项目路径")?)),
        Some("--hub") => None,
        None if bundled.is_file() => {
            player = true;
            Some(bundled)
        }
        None => None,
        Some(file) => Some(PathBuf::from(file)),
    };
    let player_project = if player {
        let path = initial
            .as_ref()
            .unwrap()
            .canonicalize()
            .context("找不到游戏项目")?;
        let p = read_project(&path)?;
        let issues = validate(&p, path.parent());
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
        Some((p, path))
    } else {
        None
    };
    let title = player_project
        .as_ref()
        .map(|(p, _)| format!("{} · OpenGalgame", p.title))
        .unwrap_or_else(|| "OpenGalgame · 项目中心".into());
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(if player { [1280., 860.] } else { [1480., 940.] })
        .with_min_inner_size(if player { [820., 620.] } else { [1100., 720.] });
    if let Some((project, path)) = &player_project
        && let Some(icon) = &project.presentation.icon
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
    let directory = directory.to_owned();
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            let app: Box<dyn eframe::App> = if let Some((project, path)) = player_project {
                Box::new(studio::Studio::new(cc, project, path, true))
            } else {
                Box::new(workspace::Workspace::new(cc, initial, directory))
            };
            Ok(app)
        }),
    )
    .map_err(|e| anyhow::anyhow!("无法建立原生窗口或图形上下文：{e}"))?;
    Ok(())
}
