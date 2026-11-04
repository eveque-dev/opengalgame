use super::*;
use opengalgame::{
    debugger::{Command, Route},
    logic,
};
impl Studio {
    pub(super) fn debug_command(&mut self, command: Command) {
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        match command.apply(&self.project, runtime) {
            Ok(()) => {
                if matches!(command, Command::Set { .. } | Command::Jump { .. }) {
                    self.v4.debug.tainted = true;
                }
                if let Some(route) = &mut self.v4.debug.route
                    && let Err(e) = route.record(command, runtime)
                {
                    self.v4.debug.running = false;
                    self.v4.debug.message = e.to_string();
                }
                self.v4.debug.observe(&self.project, runtime);
                self.advanced.scene_key.clear();
                self.auto = false;
            }
            Err(e) => {
                self.v4.debug.running = false;
                self.v4.debug.message = e.to_string();
            }
        }
    }
    pub(super) fn debug_tick(&mut self, ctx: &egui::Context) {
        if self.player_only {
            return;
        }
        if !ctx.input(|i| i.focused) || self.external_modal {
            self.v4.debug.running = false;
        }
        if self.v4.debug.running {
            if self
                .runtime
                .as_ref()
                .and_then(|r| self.project.node(&r.current.node))
                .is_some_and(|n| n.kind == NodeKind::Dialogue)
            {
                self.debug_command(Command::Advance { choice: None });
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                self.v4.debug.running = false;
            }
        }
    }
    pub(super) fn debugger_window(&mut self, ctx: &egui::Context) {
        if self.player_only || !self.v4.debug.open {
            return;
        }
        let mut open = true;
        let mut command = None;
        let mut start = false;
        let mut replay = false;
        let mut import = false;
        let mut export = false;
        egui::Window::new("试玩调试台")
            .open(&mut open)
            .constrain_to(ctx.content_rect())
            .default_pos(egui::pos2(80., 45.))
            .default_size([850., 680.])
            .vscroll(true)
            .max_height((ctx.content_rect().height() - 80.).max(420.))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("从头开始并录制").clicked() {
                        start = true;
                    }
                    if ui
                        .add_enabled(
                            self.v4.debug.route.is_some(),
                            egui::Button::new("重新复现路线"),
                        )
                        .clicked()
                    {
                        replay = true;
                    }
                    if ui.button("导入路线…").clicked() {
                        import = true;
                    }
                    if ui
                        .add_enabled(
                            self.v4.debug.route.is_some(),
                            egui::Button::new("导出路线…"),
                        )
                        .clicked()
                    {
                        export = true;
                    }
                });
                ui.label(
                    RichText::new("调试状态独立于正式工程；修改试玩变量后不写入玩家存档。")
                        .small()
                        .color(MUTED),
                );
                if self.runtime.is_none() {
                    ui.label("先从头录制，或在舞台中开始试玩。断点也可提前设置。");
                } else if let Some(runtime) = self.runtime.clone() {
                    let node = self.project.node(&runtime.current.node).cloned();
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "当前：{} · {}",
                            runtime.current.node,
                            node.as_ref()
                                .map(|n| n.title.as_str())
                                .unwrap_or("缺失节点")
                        ))
                        .size(20.)
                        .strong(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                node.as_ref().is_some_and(|n| n.kind == NodeKind::Dialogue),
                                egui::Button::new("单步前进"),
                            )
                            .clicked()
                        {
                            command = Some(Command::Advance { choice: None });
                        }
                        if ui
                            .add_enabled(!runtime.history.is_empty(), egui::Button::new("回退一步"))
                            .clicked()
                        {
                            command = Some(Command::Back);
                        }
                        if ui
                            .button(if self.v4.debug.running {
                                "暂停执行"
                            } else {
                                "运行到断点 / 选择"
                            })
                            .clicked()
                        {
                            self.v4.debug.running = !self.v4.debug.running;
                            self.auto = false;
                            ctx.request_repaint();
                        }
                        let mut breakpoint =
                            self.v4.debug.breakpoints.contains(&runtime.current.node);
                        if ui.checkbox(&mut breakpoint, "此节点断点").changed() {
                            if breakpoint {
                                self.v4
                                    .debug
                                    .breakpoints
                                    .insert(runtime.current.node.clone());
                            } else {
                                self.v4.debug.breakpoints.remove(&runtime.current.node);
                            }
                        }
                    });
                    ui.label(format!(
                        "已观察 {} / {} 节点 · 已到达 {} 个结局 · 路线 {} 操作",
                        self.v4.debug.visited.len(),
                        self.project.nodes.len(),
                        self.v4.debug.endings.len(),
                        self.v4
                            .debug
                            .route
                            .as_ref()
                            .map(|r| r.commands.len())
                            .unwrap_or(0)
                    ));
                    ui.columns(2, |columns| {
                        columns[0].heading("变量观察 / 测试值");
                        egui::ScrollArea::vertical()
                            .id_salt("debug_values")
                            .max_height(300.)
                            .show(&mut columns[0], |ui| {
                                for (key, initial) in logic::definitions(&self.project) {
                                    ui.push_id(&key, |ui| {
                                        let mut value = logic::get(&runtime.current, &key)
                                            .unwrap_or(initial.clone());
                                        let before = value.clone();
                                        ui.horizontal(|ui| {
                                            ui.add_sized(
                                                [125., 32.],
                                                egui::Label::new(RichText::new(&key).color(
                                                    if value == initial { MUTED } else { ACCENT },
                                                ))
                                                .wrap(),
                                            );
                                            logic_ui::value_editor(ui, &mut value);
                                        });
                                        if value != before {
                                            command = Some(Command::Set {
                                                variable: key.clone(),
                                                value,
                                            });
                                        }
                                    });
                                }
                            });
                        columns[1].heading("选项与隐藏原因");
                        egui::ScrollArea::vertical()
                            .id_salt("debug_choices")
                            .max_height(300.)
                            .show(&mut columns[1], |ui| {
                                if let Some(node) = &node
                                    && node.kind == NodeKind::Choice
                                {
                                    for (i, choice) in node.choices.iter().enumerate() {
                                        let available =
                                            logic::choice_available(choice, &runtime.current)
                                                .unwrap_or(false);
                                        ui.colored_label(
                                            if available { GOOD } else { WARN },
                                            format!(
                                                "{}：{}",
                                                if available { "可见" } else { "隐藏" },
                                                choice.text
                                            ),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(logic::choice_reason(
                                                    choice,
                                                    &runtime.current,
                                                ))
                                                .small(),
                                            )
                                            .wrap(),
                                        );
                                        if ui
                                            .add_enabled(
                                                available,
                                                egui::Button::new(format!("执行选项 {}", i + 1)),
                                            )
                                            .clicked()
                                        {
                                            command = Some(Command::Advance { choice: Some(i) });
                                        }
                                        ui.separator();
                                    }
                                } else {
                                    ui.label("到达选择节点时，会逐项解释显示条件。");
                                }
                            });
                    });
                    if self.v4.debug.tainted {
                        ui.colored_label(WARN, "当前包含测试变量或调试跳转，不写入玩家存档。");
                    }
                }
                ui.separator();
                let targets = self.node_targets();
                target_combo(ui, "debug_jump", &mut self.v4.debug.jump, &targets);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.runtime.is_some()
                                && self.project.node(&self.v4.debug.jump).is_some(),
                            egui::Button::new("带当前变量跳转"),
                        )
                        .clicked()
                    {
                        command = Some(Command::Jump {
                            node: self.v4.debug.jump.clone(),
                        });
                    }
                    if ui
                        .add_enabled(
                            self.project.node(&self.v4.debug.jump).is_some(),
                            egui::Button::new("切换目标断点"),
                        )
                        .clicked()
                    {
                        let id = self.v4.debug.jump.clone();
                        if !self.v4.debug.breakpoints.remove(&id) {
                            self.v4.debug.breakpoints.insert(id);
                        }
                    }
                });
                ui.label(format!(
                    "断点：{}",
                    self.v4
                        .debug
                        .breakpoints
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("、")
                ));
                if !self.v4.debug.message.is_empty() {
                    ui.colored_label(ACCENT, &self.v4.debug.message);
                }
            });
        self.v4.debug.open &= open;
        if start {
            self.begin(false);
        }
        if let Some(command) = command {
            self.debug_command(command);
        }
        if export
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("路线 JSON", &["json"])
                .set_file_name("route.json")
                .save_file()
        {
            let result =
                write_json(&path, &self.v4.debug.route).map(|_| "路线已导出，含初始状态和操作记录");
            self.message(result);
        }
        if import
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("路线 JSON", &["json"])
                .pick_file()
        {
            let result = (|| -> anyhow::Result<Route> {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 20 * 1024 * 1024,
                    "路线文件超过 20 MB"
                );
                Ok(serde_json::from_slice(&std::fs::read(path)?)?)
            })();
            match result {
                Ok(route) => {
                    self.v4.debug.route = Some(route);
                    replay = true;
                }
                Err(e) => self.v4.debug.message = e.to_string(),
            }
        }
        if replay && let Some(route) = &self.v4.debug.route {
            match route.replay_with_coverage(&self.project) {
                Ok((runtime, visited)) => {
                    self.runtime = Some(runtime);
                    self.section = Section::Script;
                    self.graph = false;
                    self.auto = false;
                    self.advanced.title_screen = false;
                    self.play_fingerprint = fingerprint(&self.project);
                    self.v4.debug.running = false;
                    self.v4.debug.tainted = true;
                    self.v4.debug.endings = visited
                        .iter()
                        .filter(|id| {
                            self.project
                                .node(id)
                                .is_some_and(|n| n.kind == NodeKind::Ending)
                        })
                        .cloned()
                        .collect();
                    self.v4.debug.visited = visited;
                    self.v4.debug.message = "路线复现一致：节点、变量和回退历史均匹配。".into();
                    self.advanced.scene_key.clear();
                }
                Err(e) => self.v4.debug.message = e.to_string(),
            }
        }
    }
}
