#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use anyhow::{Context, Result, ensure};
use opengalgame::{
    bundle::{self, Format},
    model::{Severity, safe_asset, validate},
    project_io::*,
    studio, workspace,
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
    if let Some(command @ ("--apk-check" | "--apk-setup")) = args.first().map(String::as_str) {
        use opengalgame::bundle::apk::{Dependency, InstallProgress, Setup};
        let setup = if command == "--apk-setup" {
            let accepted = args.iter().any(|arg| arg == "--accept-android-sdk-license");
            let cancel = std::sync::atomic::AtomicBool::new(false);
            Setup::install(&exe, accepted, &cancel, |progress| match progress {
                InstallProgress::Download {
                    dependency,
                    received: 0,
                } => println!("下载 {}", dependency.label()),
                InstallProgress::Extract(dependency) => {
                    println!("已校验，正在解包 {}", dependency.label())
                }
                _ => {}
            })?
        } else {
            Setup::detect(&exe)?
        };
        for dependency in Dependency::ALL {
            if let Some(path) = setup.location(dependency) {
                println!("OK {} {}", dependency.label(), path.display());
            } else {
                println!("待安装 {}", dependency.label());
            }
        }
        ensure!(setup.missing().next().is_none(), "APK 环境还未就绪");
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--test") {
        let path = PathBuf::from(
            args.get(1)
                .context("--test 需要 project.json report.json")?,
        );
        let output = PathBuf::from(args.get(2).context("--test 需要报告输出路径")?);
        let p = read_project(&path)?;
        let errors: Vec<_> = validate(&p, path.parent())
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| i.message)
            .collect();
        ensure!(errors.is_empty(), "项目校验失败：{}", errors.join("；"));
        let report = opengalgame::testing::run(&p, 8192);
        write_json(&output, &report)?;
        ensure!(
            report.passed(),
            "回归测试或分支执行失败，详见 {}",
            output.display()
        );
        println!(
            "PASS: {} states, {} endings, {} cases",
            report.states,
            report.endings.len(),
            report.cases.len()
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--replay") {
        let path = PathBuf::from(
            args.get(1)
                .context("--replay 需要 project.json route.json")?,
        );
        let route_path = PathBuf::from(args.get(2).context("--replay 需要路线 JSON")?);
        ensure!(
            std::fs::metadata(&route_path)?.len() <= 20 * 1024 * 1024,
            "路线文件超过 20 MB"
        );
        let p = read_project(&path)?;
        let route: opengalgame::debugger::Route =
            serde_json::from_slice(&std::fs::read(route_path)?)?;
        route.replay(&p)?;
        println!("PASS: route replay");
        return Ok(());
    }
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
    let command = args.first().map(String::as_str);
    if let Some(command @ ("--export" | "--export-exe" | "--export-zip" | "--export-apk")) = command
    {
        let path = PathBuf::from(args.get(1).context("--export 需要项目路径和新输出目录")?);
        let dest = PathBuf::from(args.get(2).context("--export 需要输出目录")?);
        let p = read_project(&path)?;
        let base = path.parent().unwrap_or(Path::new("."));
        let format = match command {
            "--export-exe" => Some(Format::Exe),
            "--export-zip" => Some(Format::Zip),
            "--export-apk" => Some(Format::Apk),
            _ => None,
        };
        if let Some(format) = format {
            bundle::export(&p, base, &exe, &dest, format)?;
        } else {
            export_game(&p, base, &exe, &dest)?;
        }
        println!("Exported {}", dest.display());
        return Ok(());
    }
    let embedded = if command.is_none() || command == Some("--verify-bundle") {
        bundle::open_exe(&exe)?
    } else {
        None
    };
    if command == Some("--verify-bundle") {
        let directory = embedded.as_ref().context("此 EXE 没有内嵌游戏")?;
        let p = read_project(&directory.path().join("game.json"))?;
        check_player(&p, directory.path())?;
        let report = opengalgame::testing::run(&p, 8192);
        if let Some(path) = args.get(1) {
            write_json(Path::new(path), &report)?;
        }
        ensure!(report.passed(), "内嵌游戏的分支或回归用例执行失败");
        println!(
            "PASS: {} nodes, {} endings",
            p.nodes.len(),
            report.endings.len()
        );
        return Ok(());
    }
    let mut player = false;
    let initial = match command {
        Some("--play") => {
            player = true;
            Some(PathBuf::from(args.get(1).context("--play 需要项目路径")?))
        }
        Some("--editor") => Some(PathBuf::from(args.get(1).context("--editor 需要项目路径")?)),
        Some("--hub") => None,
        None if embedded.is_some() => {
            player = true;
            embedded.as_ref().map(|dir| dir.path().join("game.json"))
        }
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
        check_player(&p, path.parent().unwrap_or(Path::new(".")))?;
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

fn check_player(project: &opengalgame::model::Project, base: &Path) -> Result<()> {
    let issues = validate(project, Some(base));
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
    Ok(())
}
