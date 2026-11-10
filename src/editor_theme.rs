use eframe::egui::{self, Color32};
#[cfg(windows)]
use eframe::egui::{RichText, Stroke};
#[cfg(windows)]
use serde::{Deserialize, Serialize};

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct EditorTheme {
    accent: [u8; 3],
    background: [u8; 3],
}

#[cfg(windows)]
impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            accent: [169, 156, 241],
            background: [21, 29, 43],
        }
    }
}

#[cfg(windows)]
impl EditorTheme {
    fn apply(self, ctx: &egui::Context) {
        ctx.style_mut(|style| self.set_visuals(&mut style.visuals));
    }

    fn set_visuals(self, visuals: &mut egui::Visuals) {
        let rgb = self.background;
        let peak = *rgb.iter().max().unwrap_or(&0);
        let dark = rgb.map(|channel| {
            if peak > 62 {
                (u16::from(channel) * 62 / u16::from(peak)) as u8
            } else {
                channel
            }
        });
        let surface = |offset: [u8; 3]| {
            Color32::from_rgb(
                dark[0] + offset[0],
                dark[1] + offset[1],
                dark[2] + offset[2],
            )
        };
        let base = surface([0, 0, 0]);
        let panel = surface([11, 13, 14]);
        let field = surface([20, 25, 31]);
        let ink = Color32::from_rgb(230, 234, 243);
        let raw_accent = Color32::from_rgb(self.accent[0], self.accent[1], self.accent[2]);
        let mut accent = raw_accent;
        while contrast(accent, field) < 4.5 {
            accent = blend(accent, Color32::WHITE, 0.12);
        }
        let mut selection = blend(field, raw_accent, 0.29);
        while contrast(ink, selection) < 4.5 {
            selection = blend(selection, base, 0.18);
        }
        visuals.panel_fill = panel;
        visuals.window_fill = panel;
        visuals.extreme_bg_color = base;
        visuals.faint_bg_color = field;
        visuals.override_text_color = Some(ink);
        visuals.widgets.inactive.bg_fill = field;
        visuals.widgets.inactive.weak_bg_fill = field;
        visuals.widgets.hovered.bg_fill = blend(field, raw_accent, 0.13);
        visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
        visuals.widgets.active.bg_fill = selection;
        visuals.widgets.active.weak_bg_fill = selection;
        visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, accent);
        visuals.selection.bg_fill = selection;
        visuals.selection.stroke = Stroke::new(1.5_f32, accent);
        visuals.hyperlink_color = accent;
    }
}

#[cfg(windows)]
fn blend(a: Color32, b: Color32, amount: f32) -> Color32 {
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
    };
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

fn contrast(a: Color32, b: Color32) -> f32 {
    fn luminance(color: Color32) -> f32 {
        let linear = |channel| {
            let srgb = f32::from(channel) / 255.;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.stroke.color
}

pub fn text_on(fill: Color32) -> Color32 {
    if contrast(fill, Color32::BLACK) >= contrast(fill, Color32::WHITE) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

#[cfg(windows)]
pub fn primary_button(ui: &egui::Ui, label: &str) -> egui::Button<'static> {
    let color = accent(ui);
    egui::Button::new(RichText::new(label).color(text_on(color))).fill(color)
}

#[cfg(windows)]
pub fn feedback(success: bool) -> Color32 {
    if success {
        Color32::from_rgb(128, 200, 179)
    } else {
        Color32::from_rgb(255, 176, 181)
    }
}

#[cfg(windows)]
pub struct Appearance {
    pub open: bool,
    theme: EditorTheme,
    message: Option<(bool, String)>,
}

#[cfg(windows)]
impl Appearance {
    fn path() -> std::path::PathBuf {
        opengalgame::paths::data_directory("editor-theme.json")
    }

    pub fn load(ctx: &egui::Context) -> Self {
        let theme = std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<EditorTheme>(&bytes).ok())
            .unwrap_or_default();
        theme.apply(ctx);
        Self {
            open: false,
            theme,
            message: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let before = self.theme;
        egui::Window::new("编辑器外观")
            .open(&mut self.open)
            .default_width(440.)
            .show(ctx, |ui| {
                ui.label("这套配色应用于项目中心和编辑工作台。作品画面在「主题」中调整。");
                ui.horizontal_wrapped(|ui| {
                    for (name, theme) in [
                        ("星夜紫", EditorTheme::default()),
                        (
                            "松林绿",
                            EditorTheme {
                                accent: [114, 218, 177],
                                background: [19, 37, 32],
                            },
                        ),
                        (
                            "暖沙金",
                            EditorTheme {
                                accent: [242, 190, 111],
                                background: [39, 30, 23],
                            },
                        ),
                    ] {
                        if ui.selectable_label(self.theme == theme, name).clicked() {
                            self.theme = theme;
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let label = ui.label("主色");
                    ui.color_edit_button_srgb(&mut self.theme.accent)
                        .labelled_by(label.id);
                    let label = ui.label("工作区底色");
                    ui.color_edit_button_srgb(&mut self.theme.background)
                        .labelled_by(label.id);
                });
                ui.label(
                    RichText::new("即时预览。编辑器会调整明暗，保留深色背景和清晰的文字对比。")
                        .small()
                        .weak(),
                );
                if ui.add(primary_button(ui, "保存配色")).clicked() {
                    self.message = Some(
                        match opengalgame::project_io::write_json(&Self::path(), &self.theme) {
                            Ok(()) => (true, "配色已保存到本机。".into()),
                            Err(error) => (false, format!("保存失败。{error}")),
                        },
                    );
                }
                if let Some((ok, message)) = &self.message {
                    ui.colored_label(feedback(*ok), message);
                }
            });
        if self.theme != before {
            self.message = None;
            self.theme.apply(ctx);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn extreme_custom_colors_preserve_readable_text() {
        for background in [[0, 0, 0], [255, 255, 255], [255, 0, 255]] {
            for accent in [[0, 0, 0], [255, 255, 255], [0, 0, 255]] {
                let mut visuals = egui::Visuals::dark();
                EditorTheme { accent, background }.set_visuals(&mut visuals);
                let ink = visuals.override_text_color.unwrap();
                let emphasis = visuals.selection.stroke.color;
                assert!(contrast(ink, visuals.selection.bg_fill) >= 4.5);
                assert!(contrast(emphasis, visuals.faint_bg_color) >= 4.5);
                assert!(contrast(text_on(emphasis), emphasis) >= 4.5);
            }
        }
    }
}
