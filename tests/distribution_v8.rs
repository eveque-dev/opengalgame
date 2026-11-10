use image::{Rgb, RgbImage, Rgba, RgbaImage};
use opengalgame::project_io::{import_asset, import_portrait};

#[test]
fn portrait_import_keys_a_solid_jpeg_without_touching_the_original() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("portrait.jpg");
    let mut portrait = RgbImage::from_pixel(96, 128, Rgb([244, 244, 244]));
    for y in 25..128 {
        for x in 26..69 {
            portrait[(x, y)] = Rgb([40, 45, 104]);
        }
    }
    for y in 60..80 {
        for x in 39..56 {
            portrait[(x, y)] = Rgb([244, 244, 244]);
        }
    }
    for y in 111..128 {
        for x in 0..96 {
            portrait[(x, y)] = Rgb([40, 45, 104]);
        }
    }
    portrait.save(&source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let file = import_portrait(work.path(), &source).unwrap();
    assert!(file.ends_with(".png"));
    let image = image::open(work.path().join(&file)).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (96, 128));
    assert_eq!(image[(10, 10)][3], 0);
    assert_eq!(image[(30, 100)][3], 255);
    assert_eq!(image[(0, 127)][3], 255);
    assert_eq!(image[(46, 70)][3], 255);
    assert_eq!(import_portrait(work.path(), &source).unwrap(), file);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let background = import_asset(work.path(), &source).unwrap();
    assert_eq!(
        std::fs::read(work.path().join(background)).unwrap(),
        original
    );
}

#[test]
fn portrait_import_preserves_existing_alpha_and_uncertain_backgrounds() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("portrait.png");
    for image in [
        RgbaImage::from_fn(48, 48, |x, y| {
            Rgba([255, 255, 255, if x < y { 255 } else { 0 }])
        }),
        RgbaImage::from_fn(48, 48, |x, y| Rgba([(x * 5) as u8, (y * 5) as u8, 43, 255])),
        RgbaImage::from_pixel(48, 48, Rgba([0, 180, 45, 255])),
    ] {
        image.save(&source).unwrap();
        assert_eq!(
            import_portrait(work.path(), &source).unwrap(),
            import_asset(work.path(), &source).unwrap()
        );
    }
}

#[cfg(windows)]
#[test]
fn exe_and_zip_exports_contain_playable_assets_and_never_overwrite() {
    use opengalgame::{
        bundle::{Format, export, open_exe},
        model::validate,
        project_io::read_project,
    };
    use std::{fs::File, path::Path, process::Command};

    let work = tempfile::tempdir().unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tidal-stage");
    let mut project = read_project(&base.join("project.json")).unwrap();
    project.story_recap = "作者私用资料".into();
    let editor = Path::new(env!("CARGO_BIN_EXE_opengalgame"));
    assert!(open_exe(editor).unwrap().is_none());
    let exe = work.path().join("内嵌游戏.exe");
    export(&project, &base, editor, &exe, Format::Exe).unwrap();
    let embedded = open_exe(&exe).unwrap().unwrap();
    let playable = read_project(&embedded.path().join("game.json")).unwrap();
    assert!(playable.story_recap.is_empty());
    assert!(validate(&playable, Some(embedded.path())).is_empty());
    assert!(
        Command::new(&exe)
            .arg("--verify-bundle")
            .status()
            .unwrap()
            .success()
    );
    assert!(export(&project, &base, editor, &exe, Format::Exe).is_err());

    let zip = work.path().join("传统游戏.zip");
    export(&project, &base, editor, &zip, Format::Zip).unwrap();
    let mut archive = zip::ZipArchive::new(File::open(zip).unwrap()).unwrap();
    assert!(archive.by_name("Game.exe").is_ok());
    let unpacked = work.path().join("unpacked");
    archive.extract(&unpacked).unwrap();
    let p = read_project(&unpacked.join("game.json")).unwrap();
    assert!(validate(&p, Some(&unpacked)).is_empty());
    assert_eq!(p.nodes.len(), playable.nodes.len());
}
