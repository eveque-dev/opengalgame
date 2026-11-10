use super::*;
use opengalgame::{
    assets::{self, AssetKind},
    production::*,
    testing,
};
impl Studio {
    pub(super) fn production_tick(&mut self, ctx: &egui::Context) {
        self.advanced.sound.duck_voice = self.advanced.settings.duck_voice;
        self.advanced.sound.set_levels(
            self.advanced.settings.music,
            self.advanced.settings.ambience,
            self.advanced.settings.voice,
            self.advanced.settings.effects,
        );
        if self.advanced.sound.fading() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        self.ensure_game_font(ctx);
        if let Some(rx) = &self.v5.report_rx {
            match rx.try_recv() {
                Ok(report) => {
                    self.v5.report = Some(report);
                    self.v5.report_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.v5.report_rx = None;
                    self.status = "测试任务中断，请重新运行。".into();
                }
                _ => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }
        if self.v5.layout != self.v5.persisted
            && self.v5.layout_at.elapsed() > Duration::from_millis(800)
        {
            if write_json(&v5::V5::layout_path(&self.path), &self.v5.layout).is_ok() {
                self.v5.persisted = self.v5.layout.clone();
            }
            self.v5.layout_at = Instant::now();
        }
        if self.v5.layout != self.v5.persisted {
            ctx.request_repaint_after(Duration::from_millis(850));
        }
    }
    pub(super) fn layout_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("布局与面板", |ui| {
            ui.checkbox(&mut self.v5.layout.outline, "显示剧情目录");
            ui.checkbox(&mut self.v5.layout.inspector, "显示场景属性");
            if ui.button("宽屏条件编辑器").clicked() {
                self.v5.logic_open = true;
                ui.close();
            }
            if ui.button("恢复默认布局").clicked() {
                self.v5.layout = Default::default();
                ui.ctx().memory_mut(|m| {
                    m.data
                        .remove::<egui::containers::panel::PanelState>(egui::Id::new((
                            "scenes", &self.path,
                        )));
                    m.data
                        .remove::<egui::containers::panel::PanelState>(egui::Id::new((
                            "properties",
                            self.path.clone(),
                        )));
                });
                ui.close();
            }
            ui.label("面板宽度、开关和表格列宽保存在本机，按项目分别恢复。");
        });
        if ui.button("宽屏条件").clicked() {
            self.v5.logic_open = true;
        }
    }
    pub(super) fn theme_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("游戏界面主题");
        let mut enabled = self.project.production.theme.is_some();
        if ui
            .checkbox(&mut enabled, "为这部作品启用独立主题")
            .changed()
        {
            self.project.production.theme = enabled.then(GameTheme::default);
        }
        if !enabled {
            ui.label("启用后，可分别调整对话框、姓名框、选项、字体、标题菜单和存档页。");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("快速起点");
            for i in 0..3 {
                let preset = GameTheme::preset(i);
                if ui.button(&preset.name).clicked() {
                    self.project.production.theme = Some(preset);
                }
            }
        });
        let base = self.base().to_owned();
        if ui
            .add(
                egui::Slider::new(&mut self.project.presentation.font_size, 16.0..=36.0)
                    .text("正文默认字号"),
            )
            .changed()
        {
            self.text_size = self.project.presentation.font_size;
            self.advanced.settings.font = self.text_size;
        }
        egui::ScrollArea::vertical()
            .id_salt("theme_editor")
            .show(ui, |ui| {
                if let Some(theme) = &mut self.project.production.theme {
                    ui.horizontal(|ui| {
                        ui.label("主题名称");
                        ui.text_edit_singleline(&mut theme.name);
                    });
                    ui.columns(2, |columns| {
                        let ui = &mut columns[0];
                        color(ui, "对话框底色", &mut theme.panel);
                        color(ui, "正文颜色", &mut theme.text);
                        color(ui, "姓名颜色", &mut theme.name_color);
                        ui.add(
                            egui::Slider::new(&mut theme.opacity, 80..=255).text("对话框不透明度"),
                        );
                        ui.add(
                            egui::Slider::new(&mut theme.panel_height, 0.25..=0.75)
                                .text("对话框高度比例"),
                        );
                        ui.add(
                            egui::Slider::new(&mut theme.panel_width, 0.6..=0.98)
                                .text("对话框宽度比例"),
                        );
                        ui.add(egui::Slider::new(&mut theme.radius, 0..=32).text("圆角"));
                        ui.checkbox(&mut theme.name_box, "显示姓名框底色");
                        ui.add(
                            egui::Slider::new(&mut theme.name_size, 14.0..=40.0).text("姓名字号"),
                        );
                        let ui = &mut columns[1];
                        color(ui, "选项底色", &mut theme.choice);
                        color(ui, "选项文字", &mut theme.choice_text);
                        color(ui, "标题 / 存档页底色", &mut theme.menu);
                        color(ui, "标题主按钮颜色", &mut self.project.presentation.accent);
                        ui.add(
                            egui::Slider::new(&mut theme.button_height, 28.0..=72.0)
                                .text("选项按钮高度"),
                        );
                        ui.add(
                            egui::Slider::new(&mut theme.choice_size, 14.0..=36.0).text("选项字号"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label("标题菜单位置");
                            for side in [
                                PortraitSide::Left,
                                PortraitSide::Center,
                                PortraitSide::Right,
                            ] {
                                ui.selectable_value(&mut theme.menu_position, side, side.label());
                            }
                        });
                        ui.add(
                            egui::Slider::new(&mut theme.save_columns, 1..=3).text("存档页列数"),
                        );
                        egui::ComboBox::from_id_salt("game_font")
                            .selected_text(&theme.font_family)
                            .show_ui(ui, |ui| {
                                for family in ["微软雅黑", "宋体", "黑体"] {
                                    if ui
                                        .selectable_value(
                                            &mut theme.font_family,
                                            family.into(),
                                            family,
                                        )
                                        .changed()
                                    {
                                        theme.font_file = None;
                                    }
                                }
                            });
                        if ui.button("导入 TTF / OTF / TTC 字体…").clicked()
                            && let Some(file) = rfd::FileDialog::new()
                                .add_filter("字体", &["ttf", "otf", "ttc"])
                                .pick_file()
                        {
                            match assets::import_font(&base, &file) {
                                Ok(relative) => {
                                    theme.font_family = "自定义".into();
                                    theme.font_file = Some(relative);
                                }
                                Err(e) => {
                                    self.status = e.to_string();
                                    self.status_error = true;
                                }
                            }
                        }
                        if let Some(file) = &theme.font_file {
                            ui.label(file);
                        }
                    });
                }
                ui.separator();
                ui.strong("对白与选项实时预览");
                let width = ui.available_width().min(1000.);
                self.scene(ui, Vec2::new(width, (width * 9. / 16.).min(380.)), false);
                if let Some(theme) = &self.project.production.theme {
                    ui.horizontal_wrapped(|ui| {
                        egui::Frame::new()
                            .fill(rgb(theme.menu))
                            .inner_margin(16)
                            .show(ui, |ui| {
                                ui.label(RichText::new("标题菜单").color(rgb(theme.text)));
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(
                                        RichText::new("开始故事").color(rgb(theme.choice_text)),
                                    )
                                    .fill(rgb(theme.choice)),
                                );
                            });
                        egui::Frame::new()
                            .fill(rgb(theme.menu))
                            .inner_margin(16)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("存档页 · {} 列", theme.save_columns))
                                        .color(rgb(theme.text)),
                                );
                                ui.horizontal(|ui| {
                                    for i in 0..theme.save_columns {
                                        ui.add_enabled(
                                            false,
                                            egui::Button::new(format!("存档 {}", i + 1))
                                                .fill(rgb(theme.choice)),
                                        );
                                    }
                                });
                            });
                    });
                }
            });
    }
    pub(super) fn test_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("分支与回归测试");
        ui.label(
            "自动探索检查从开场可达的节点、选项与结局；保存的用例验证录制时预期的最终节点和变量。",
        );
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.v5.test_limit)
                    .range(1..=8192)
                    .prefix("状态上限 "),
            );
            if ui
                .add_enabled(
                    self.v5.report_rx.is_none(),
                    egui::Button::new("运行全部测试"),
                )
                .clicked()
            {
                let p = self.project.clone();
                let limit = self.v5.test_limit;
                let ctx = ui.ctx().clone();
                let (tx, rx) = std::sync::mpsc::channel();
                self.v5.report_rx = Some(rx);
                std::thread::spawn(move || {
                    let report = testing::run(&p, limit);
                    let _ = tx.send(report);
                    ctx.request_repaint();
                });
            }
            if self.v5.report_rx.is_some() {
                ui.spinner();
                ui.label("正在后台检查…");
            }
            if ui
                .add_enabled(
                    self.v5.report.is_some(),
                    egui::Button::new("导出报告 JSON…"),
                )
                .clicked()
                && let Some(file) = rfd::FileDialog::new()
                    .set_file_name("test-report.json")
                    .save_file()
            {
                let result = write_json(&file, &self.v5.report).map(|_| "测试报告已导出");
                self.message(result);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("用例名称");
            ui.text_edit_singleline(&mut self.v5.case_name);
            if ui
                .add_enabled(
                    self.v4.debug.route.is_some() && self.project.production.cases.len() < 256,
                    egui::Button::new("保存本次试玩路线为用例"),
                )
                .clicked()
                && let Some(route) = &self.v4.debug.route
            {
                self.project
                    .production
                    .cases
                    .push(testing::TestCase::from_route(
                        self.v5.case_name.clone(),
                        route,
                        &self.project,
                    ));
            }
        });
        let mut remove = None;
        for (i, case) in self.project.production.cases.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {} 操作 → {}",
                    case.name,
                    case.commands.len(),
                    case.expected_node
                ));
                if ui.button("移除此用例").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.project.production.cases.remove(i);
        }
        let Some(report) = self.v5.report.clone() else {
            return;
        };
        if report.fingerprint != fingerprint(&self.project) {
            ui.colored_label(WARN, "工程已变化，此报告属于上次版本，请重新运行。");
        }
        ui.separator();
        ui.colored_label(
            if report.passed() { GOOD } else { BAD },
            format!(
                "{} · {} 个状态 · {}/{} 节点 · {} 个结局 · {}/{} 用例通过",
                if report.passed() {
                    "执行通过"
                } else {
                    "发现问题"
                },
                report.states,
                report.nodes.len(),
                self.project.nodes.len(),
                report.endings.len(),
                report.cases.iter().filter(|c| c.passed).count(),
                report.cases.len()
            ),
        );
        if report.truncated {
            ui.colored_label(
                WARN,
                "已到达状态数或路径长度上限；此报告不代表全部数值循环均已穷举。",
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("test_report")
            .show(ui, |ui| {
                for c in &report.cases {
                    ui.colored_label(
                        if c.passed { GOOD } else { BAD },
                        format!(
                            "{} · {}：{}",
                            if c.passed { "通过" } else { "失败" },
                            c.name,
                            c.message
                        ),
                    );
                }
                for error in &report.errors {
                    ui.colored_label(BAD, error);
                }
                for (label, ids) in [
                    ("尚未覆盖的节点", &report.uncovered_nodes),
                    ("尚未抵达的结局", &report.uncovered_endings),
                ] {
                    ui.collapsing(format!("{label} · {}", ids.len()), |ui| {
                        for id in ids {
                            if ui.button(id).clicked()
                                && let Some(index) =
                                    self.project.nodes.iter().position(|n| &n.id == id)
                            {
                                self.select(index);
                                self.section = Section::Script;
                            }
                        }
                    });
                }
                ui.collapsing(
                    format!("尚未执行的选项 · {}", report.uncovered_choices.len()),
                    |ui| {
                        for (id, index) in &report.uncovered_choices {
                            ui.label(format!("{id} · 选项 {}", index + 1));
                        }
                    },
                );
                ui.collapsing("自动找到的结局路线", |ui| {
                    for case in &report.routes {
                        ui.horizontal(|ui| {
                            ui.label(&case.name);
                            if ui
                                .add_enabled(
                                    self.project.production.cases.len() < 256,
                                    egui::Button::new("保存为用例"),
                                )
                                .clicked()
                            {
                                self.project.production.cases.push(case.clone());
                            }
                        });
                    }
                });
            });
    }
    pub(super) fn assets_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("素材维护");
        ui.horizontal_wrapped(|ui| {
            if ui.button("刷新引用与包体").clicked() || self.v5.audit.is_none() {
                match assets::audit(&self.project, self.base()) {
                    Ok(a) => self.v5.audit = Some(a),
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
            if ui.button("批量定位缺失素材…").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                match assets::preview_repair(&self.project, self.base(), &dir) {
                    Ok(plan) => self.v5.repair = Some(plan),
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
            if ui.button("批量匹配逐句配音…").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                match assets::voice_matches(&self.project, &dir) {
                    Ok((matched, ambiguous)) => {
                        self.v5.voices = Some((fingerprint(&self.project), matched, ambiguous))
                    }
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
        });
        ui.label(
            "配音文件以节点标识命名，例如 opening.wav。同名候选不唯一时保留待处理，不会自动覆盖。",
        );
        ui.collapsing("修改素材标识并同步引用", |ui| {
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("rename_asset_kind")
                    .selected_text(match self.v5.asset_kind {
                        AssetKind::Character => "人物",
                        AssetKind::Background => "背景",
                        AssetKind::Audio => "音频",
                        AssetKind::Font => "字体",
                    })
                    .show_ui(ui, |ui| {
                        for (kind, label) in [
                            (AssetKind::Character, "人物"),
                            (AssetKind::Background, "背景"),
                            (AssetKind::Audio, "音频"),
                        ] {
                            if ui
                                .selectable_value(&mut self.v5.asset_kind, kind, label)
                                .changed()
                            {
                                self.v5.asset_old.clear();
                            }
                        }
                    });
                let list: Vec<_> = match self.v5.asset_kind {
                    AssetKind::Character => self
                        .project
                        .characters
                        .iter()
                        .map(|c| (c.id.clone(), c.name.clone()))
                        .collect(),
                    AssetKind::Background => self
                        .project
                        .backgrounds
                        .iter()
                        .map(|b| (b.id.clone(), b.name.clone()))
                        .collect(),
                    _ => self
                        .project
                        .audio
                        .iter()
                        .map(|a| (a.id.clone(), a.name.clone()))
                        .collect(),
                };
                egui::ComboBox::from_id_salt("asset_id_old")
                    .selected_text(if self.v5.asset_old.is_empty() {
                        "选择素材"
                    } else {
                        &self.v5.asset_old
                    })
                    .show_ui(ui, |ui| {
                        for (id, name) in list {
                            ui.selectable_value(
                                &mut self.v5.asset_old,
                                id.clone(),
                                format!("{id} · {name}"),
                            );
                        }
                    });
                ui.add(egui::TextEdit::singleline(&mut self.v5.asset_new).hint_text("新标识"));
                if ui.button("更新标识与引用").clicked() {
                    match assets::rename_id(
                        &self.project,
                        self.v5.asset_kind,
                        &self.v5.asset_old,
                        self.v5.asset_new.trim(),
                    ) {
                        Ok(p) => {
                            self.project = p;
                            self.v5.audit = None;
                            self.v5.asset_old = self.v5.asset_new.trim().into();
                            self.message(Ok("素材标识及演出引用已同步，可撤销。"));
                        }
                        Err(e) => self.message(Err::<String, _>(e)),
                    }
                }
            });
        });
        let Some(audit) = self.v5.audit.clone() else {
            return;
        };
        ui.label(format!(
            "引用文件 {} 个 · 缺失 {} 个 · 素材导出占用 {:.2} MB · 未引用磁盘文件 {:.2} MB",
            audit.files.len(),
            audit.files.iter().filter(|f| f.missing).count(),
            audit.referenced_bytes as f64 / 1048576.,
            audit.unused_bytes as f64 / 1048576.
        ));
        ui.collapsing(
            "另存文件名并同步引用（保留原文件供撤销）",
            |ui| {
                egui::ComboBox::from_id_salt("rename_file")
                    .selected_text(if self.v5.rename_file.is_empty() {
                        "选择引用文件"
                    } else {
                        &self.v5.rename_file
                    })
                    .show_ui(ui, |ui| {
                        for f in &audit.files {
                            ui.selectable_value(&mut self.v5.rename_file, f.file.clone(), &f.file);
                        }
                    });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.v5.file_stem)
                            .hint_text("新文件名，不含扩展名"),
                    );
                    if ui.button("另存并更新引用").clicked() {
                        match assets::rename_file(
                            &self.project,
                            self.base(),
                            &self.v5.rename_file,
                            &self.v5.file_stem,
                        ) {
                            Ok(p) => {
                                self.project = p;
                                self.v5.audit = None;
                                self.images.clear();
                                self.message(Ok("新文件已保存，引用已更新，可撤销。"));
                            }
                            Err(e) => self.message(Err::<String, _>(e)),
                        }
                    }
                });
            },
        );
        egui::ScrollArea::vertical()
            .id_salt("asset_audit")
            .show(ui, |ui| {
                for file in &audit.files {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if file.missing { BAD } else { GOOD },
                            if file.missing { "缺失" } else { "正常" },
                        );
                        ui.label(format!(
                            "{} · {:.1} KB · {}",
                            file.file,
                            file.bytes as f64 / 1024.,
                            file.owners.join("，")
                        ));
                        if file.missing
                            && ui.button("重新定位…").clicked()
                            && let Some(source) = rfd::FileDialog::new().pick_file()
                        {
                            match assets::import_by_kind(file.kind, self.base(), &source) {
                                Ok(new) => {
                                    assets::replace_path(&mut self.project, &file.file, &new);
                                    self.v5.audit = None;
                                    self.images.clear();
                                }
                                Err(e) => self.message(Err::<String, _>(e)),
                            }
                        }
                    });
                }
                ui.collapsing(
                    format!(
                        "素材库中尚未用于场景的条目 · {}",
                        audit.unused_catalog.len()
                    ),
                    |ui| {
                        for name in &audit.unused_catalog {
                            ui.label(name);
                        }
                    },
                );
                ui.collapsing(
                    format!("磁盘中未被工程引用的文件 · {}", audit.unused_disk.len()),
                    |ui| {
                        ui.label("这些文件不会随作品导出；可能是旧版本或撤销保留文件。");
                        for (name, bytes) in &audit.unused_disk {
                            ui.label(format!("{name} · {:.1} KB", *bytes as f64 / 1024.));
                        }
                    },
                );
            });
    }
    pub(super) fn production_dialogs(&mut self, ctx: &egui::Context) {
        self.wide_logic(ctx);
        if let Some(plan) = self.v5.repair.clone() {
            let mut apply = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("repair_assets")).show(ctx, |ui| {
                ui.heading("预览批量修复");
                ui.label(format!(
                    "找到 {} 个唯一候选，{} 个仍需手动定位",
                    plan.replacements.len(),
                    plan.unresolved.len()
                ));
                egui::ScrollArea::vertical()
                    .max_height(300.)
                    .show(ui, |ui| {
                        for (old, _, source) in &plan.replacements {
                            ui.label(format!(
                                "{old} → {}",
                                source.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        for old in &plan.unresolved {
                            ui.colored_label(WARN, format!("待处理：{old}"));
                        }
                    });
                ui.horizontal(|ui| {
                    apply = ui
                        .add_enabled(
                            !plan.replacements.is_empty(),
                            egui::Button::new("应用匹配项"),
                        )
                        .clicked();
                    cancel = ui.button("取消").clicked();
                });
            });
            if apply {
                match assets::apply_repair(&self.project, self.base(), &plan) {
                    Ok(p) => {
                        self.project = p;
                        self.v5.audit = None;
                        self.images.clear();
                        self.v5.repair = None;
                    }
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
            if cancel {
                self.v5.repair = None;
            }
        }
        if let Some((fingerprint_before, matches, ambiguous)) = self.v5.voices.clone() {
            let mut apply = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("voice_match")).show(ctx, |ui| {
                ui.heading("预览配音匹配");
                ui.label(format!(
                    "匹配 {} 段，{} 段存在同名冲突",
                    matches.len(),
                    ambiguous.len()
                ));
                egui::ScrollArea::vertical()
                    .max_height(300.)
                    .show(ui, |ui| {
                        for (node, file) in &matches {
                            ui.label(format!(
                                "{node} → {}",
                                file.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        for node in &ambiguous {
                            ui.colored_label(WARN, format!("冲突：{node}"));
                        }
                    });
                ui.horizontal(|ui| {
                    apply = ui
                        .add_enabled(!matches.is_empty(), egui::Button::new("导入并绑定配音"))
                        .clicked();
                    cancel = ui.button("取消").clicked();
                });
            });
            if apply {
                let result = if fingerprint_before != fingerprint(&self.project) {
                    Err(anyhow::anyhow!("工程已变化，请重新预览"))
                } else {
                    assets::apply_voices(&self.project, self.base(), &matches)
                };
                match result {
                    Ok(p) => {
                        self.project = p;
                        self.v5.audit = None;
                        self.v5.voices = None;
                    }
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
            if cancel {
                self.v5.voices = None;
            }
        }
        if self.v5.export_preflight {
            let mut proceed = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("export_preflight")).show(ctx, |ui| {
                ui.heading("导出前检查");
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.v5.export_format, Format::Exe, "Windows EXE");
                    ui.selectable_value(&mut self.v5.export_format, Format::Zip, "Windows ZIP");
                    ui.selectable_value(&mut self.v5.export_format, Format::Apk, "Android APK");
                });
                ui.label(match self.v5.export_format {
                    Format::Exe => "单个 .exe 内含播放器、剧本和素材，双击游玩。",
                    Format::Zip => "保留 Game.exe、game.json 与素材目录，解压后游玩。",
                    Format::Apk => {
                        "适用于 Android 8.0 及以上的 64 位设备。导出需 JDK 及 Android SDK。"
                    }
                });
                if let Some(a) = &self.v5.audit {
                    ui.label(format!(
                        "素材占用 {:.2} MB，另加播放器和组件许可。",
                        a.referenced_bytes as f64 / 1048576.
                    ));
                    ui.label(format!(
                        "{} 个磁盘文件未引用，不会导出；{} 个素材库条目尚未用于场景。",
                        a.unused_disk.len(),
                        a.unused_catalog.len()
                    ));
                    ui.collapsing("查看未使用素材清单", |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(160.)
                            .show(ui, |ui| {
                                for (name, bytes) in &a.unused_disk {
                                    ui.label(format!(
                                        "未引用文件：{name} · {:.1} KB",
                                        *bytes as f64 / 1024.
                                    ));
                                }
                                for item in &a.unused_catalog {
                                    ui.label(format!("未用于场景：{item}"));
                                }
                            });
                    });
                    let missing = a.files.iter().filter(|f| f.missing).count();
                    if missing > 0 {
                        ui.colored_label(
                            BAD,
                            format!("有 {missing} 个素材缺失，请先在素材维护中修复。"),
                        );
                    }
                    ui.horizontal(|ui| {
                        proceed = ui
                            .add_enabled(missing == 0, egui::Button::new("选择保存位置"))
                            .clicked();
                        cancel = ui.button("返回编辑").clicked();
                    });
                } else {
                    ui.colored_label(BAD, "无法读取素材清单，请在素材维护中刷新。");
                    cancel = ui.button("返回").clicked();
                }
            });
            if proceed {
                self.v5.export_preflight = false;
                self.export_production();
            }
            if cancel {
                self.v5.export_preflight = false;
            }
        }
    }
    fn export_production(&mut self) {
        if !(self.managed || self.check()) {
            self.section = Section::Check;
            return;
        }
        let format = self.v5.export_format;
        let extension = format.extension();
        if let Some(destination) = rfd::FileDialog::new()
            .set_title("保存游戏发布包")
            .add_filter(extension.to_uppercase(), &[extension])
            .set_file_name(format!("OpenGalgame-{}.{extension}", stamp()))
            .save_file()
        {
            let destination = if destination.extension().is_none() {
                destination.with_extension(extension)
            } else {
                destination
            };
            if self.managed {
                self.workspace_request = Some(WorkspaceRequest::Export {
                    project: Box::new(self.project.clone()),
                    base: self.base().to_owned(),
                    destination,
                    format,
                });
            } else {
                let result = std::env::current_exe()
                    .map_err(anyhow::Error::from)
                    .and_then(|exe| {
                        bundle::export(&self.project, self.base(), &exe, &destination, format)
                    });
                match result {
                    Ok(path) => {
                        self.export_path = Some(path);
                        self.message(Ok("游戏已导出。"));
                    }
                    Err(e) => self.message(Err::<String, _>(e)),
                }
            }
        }
    }
}
fn color(ui: &mut egui::Ui, label: &str, value: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.color_edit_button_srgb(value);
    });
}
fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
