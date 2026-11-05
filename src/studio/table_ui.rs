use super::*;
use std::collections::BTreeSet;
pub(super) struct TableState {
    pub selected: BTreeSet<String>,
    pub collapsed: BTreeSet<String>,
    pub query: String,
    pub chapter: String,
    pub speaker: Option<String>,
    pub background: Option<String>,
    pub revision: u64,
    pub key: String,
    pub rows: Vec<Row>,
    pub preview: Option<opengalgame::spreadsheet::Preview>,
    pub active: Option<(String, usize)>,
    pub pending_focus: Option<(String, usize)>,
    pub visible: std::ops::Range<usize>,
}
#[derive(Clone)]
pub(super) enum Row {
    Chapter(String, usize),
    Node(usize),
}
impl Default for TableState {
    fn default() -> Self {
        Self {
            selected: Default::default(),
            collapsed: Default::default(),
            query: String::new(),
            chapter: String::new(),
            speaker: None,
            background: None,
            revision: u64::MAX,
            key: String::new(),
            rows: vec![],
            preview: None,
            active: None,
            pending_focus: None,
            visible: 0..0,
        }
    }
}
impl Studio {
    pub(super) fn table_editor(&mut self, ui: &mut egui::Ui) {
        let mut export = false;
        let mut import = false;
        let mut select_all = false;
        let mut clear = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading("剧本表格");
            ui.label(format!(
                "{} 段 · 已选 {} 段",
                self.project.nodes.len(),
                self.v4.table.selected.len()
            ));
            ui.add(
                egui::TextEdit::singleline(&mut self.v4.table.query)
                    .hint_text("搜索标识、章节或对白")
                    .desired_width(230.),
            );
            if ui.button("全选筛选结果").clicked() {
                select_all = true;
            }
            if ui.button("清空选择").clicked() {
                clear = true;
            }
            if ui.button("导出 CSV…").clicked() {
                export = true;
            }
            if ui.button("预览导入 CSV…").clicked() {
                import = true;
            }
        });
        ui.label(
            RichText::new("按标识更新已有节点；导入只在确认后应用。演出、选项和条件随 CSV 保留。")
                .small()
                .color(MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("批量操作");
            ui.add(
                egui::TextEdit::singleline(&mut self.v4.table.chapter)
                    .hint_text("章节名")
                    .desired_width(130.),
            );
            if ui
                .add_enabled(
                    !self.v4.table.selected.is_empty(),
                    egui::Button::new("设置章节"),
                )
                .clicked()
            {
                let selected = &self.v4.table.selected;
                for i in 0..self.project.nodes.len() {
                    if selected.contains(&self.project.nodes[i].id) {
                        self.project.nodes[i].chapter = self.v4.table.chapter.clone();
                    }
                }
            }
            let chars: Vec<_> = self
                .project
                .characters
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect();
            optional_combo(
                ui,
                "batch_speaker",
                &mut self.v4.table.speaker,
                &chars,
                "旁白",
            );
            if ui
                .add_enabled(
                    !self.v4.table.selected.is_empty(),
                    egui::Button::new("设置发言人"),
                )
                .clicked()
            {
                let selected = &self.v4.table.selected;
                for i in 0..self.project.nodes.len() {
                    if selected.contains(&self.project.nodes[i].id) {
                        let n = &mut self.project.nodes[i];
                        n.character = self.v4.table.speaker.clone();
                        n.expression = None;
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let bgs: Vec<_> = self
                .project
                .backgrounds
                .iter()
                .map(|b| (b.id.clone(), b.name.clone()))
                .collect();
            optional_combo(
                ui,
                "batch_background",
                &mut self.v4.table.background,
                &bgs,
                "纯色舞台",
            );
            if ui
                .add_enabled(
                    !self.v4.table.selected.is_empty(),
                    egui::Button::new("设置背景"),
                )
                .clicked()
            {
                let selected = &self.v4.table.selected;
                for i in 0..self.project.nodes.len() {
                    if selected.contains(&self.project.nodes[i].id) {
                        self.project.nodes[i].background = self.v4.table.background.clone();
                    }
                }
            }
            if ui.button("展开所有章节").clicked() {
                self.v4.table.collapsed.clear();
                self.v4.table.revision = u64::MAX;
            }
        });
        let key = format!("{}|{:?}", self.v4.table.query, self.v4.table.collapsed);
        if self.v4.table.revision != self.revision || self.v4.table.key != key {
            let mut chapters = Vec::<(String, Vec<usize>)>::new();
            for (i, n) in self.project.nodes.iter().enumerate() {
                if !opengalgame::authoring::matches(n, &self.v4.table.query) {
                    continue;
                }
                let chapter = if n.chapter.is_empty() {
                    "未分章".to_owned()
                } else {
                    n.chapter.clone()
                };
                if let Some((_, rows)) = chapters.iter_mut().find(|(c, _)| *c == chapter) {
                    rows.push(i);
                } else {
                    chapters.push((chapter, vec![i]));
                }
            }
            self.v4.table.rows.clear();
            for (chapter, rows) in chapters {
                self.v4
                    .table
                    .rows
                    .push(Row::Chapter(chapter.clone(), rows.len()));
                if !self.v4.table.collapsed.contains(&chapter) {
                    self.v4.table.rows.extend(rows.into_iter().map(Row::Node));
                }
            }
            self.v4.table.key = key;
            self.v4.table.revision = self.revision;
        }
        if select_all {
            self.v4.table.selected = self
                .project
                .nodes
                .iter()
                .filter(|n| opengalgame::authoring::matches(n, &self.v4.table.query))
                .map(|n| n.id.clone())
                .collect();
        }
        if clear {
            self.v4.table.selected.clear();
        }
        let chars: Vec<_> = self
            .project
            .characters
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        let bgs: Vec<_> = self
            .project
            .backgrounds
            .iter()
            .map(|b| (b.id.clone(), b.name.clone()))
            .collect();
        let targets = self.node_targets();
        let jump = self.table_grid(ui, &chars, &bgs, &targets);
        if let Some(index) = jump {
            self.select(index);
            self.section = Section::Script;
        }
        if export
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("UTF-8 CSV", &["csv"])
                .set_file_name("script.csv")
                .save_file()
        {
            let result = opengalgame::spreadsheet::export(&self.project)
                .and_then(|text| Ok(std::fs::write(path, text)?))
                .map(|_| "CSV 已导出，支持多行正文与完整演出数据");
            self.message(result);
        }
        if import
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("UTF-8 CSV", &["csv"])
                .pick_file()
        {
            match opengalgame::spreadsheet::read_csv(&path, &self.project) {
                Ok(preview) => self.v4.table.preview = Some(preview),
                Err(e) => self.message(Err::<String, _>(e)),
            }
        }
    }
    fn table_grid(
        &mut self,
        ui: &mut egui::Ui,
        chars: &[(String, String)],
        bgs: &[(String, String)],
        targets: &[(String, String)],
    ) -> Option<usize> {
        let ids: Vec<String> = self
            .v4
            .table
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Node(i) => Some(self.project.nodes[*i].id.clone()),
                _ => None,
            })
            .collect();
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(!self.v4.table.selected.is_empty(),egui::Button::new("复制所选行")).clicked(){
                let selected:Vec<_>=ids.iter().filter(|id|self.v4.table.selected.contains(*id)).cloned().collect();
                match opengalgame::spreadsheet::copy_grid(&self.project,&selected){Ok(text)=>ui.ctx().copy_text(text),Err(e)=>self.message(Err::<String,_>(e))}
            }
            ui.label(RichText::new("文本格：Tab / Shift+Tab 换格，Enter 换行，Shift+Enter 写正文换行；含制表符的粘贴按单元格应用。").small().color(MUTED));
            if ui.small_button("重置列宽").clicked(){self.v5.layout.columns=v5::Layout::default().columns;}
        });
        if self.runtime.is_none()
            && ui.is_enabled()
            && let Some((id, col)) = self.v4.table.active.clone()
        {
            let focused = ui
                .ctx()
                .memory(|m| m.has_focus(cell_id(&self.path, &id, col)));
            if focused {
                let paste = ui.ctx().input_mut(|input| {
                    let index = input
                        .events
                        .iter()
                        .position(|e| matches!(e,egui::Event::Paste(text) if text.contains('\t')));
                    index.and_then(|i| match input.events.remove(i) {
                        egui::Event::Paste(text) => Some(text),
                        _ => None,
                    })
                });
                if let Some(text) = paste
                    && let Some(start) = ids.iter().position(|n| n == &id)
                {
                    let column = match col {
                        1 => 0,
                        3 => 2,
                        5 => 4,
                        _ => 0,
                    };
                    match opengalgame::spreadsheet::paste_grid(
                        &self.project,
                        &ids[start..],
                        column,
                        &text,
                    ) {
                        Ok(project) => {
                            self.project = project;
                            self.last_edit = Instant::now() - Duration::from_secs(2);
                            self.message(Ok("表格粘贴已应用，可撤销。"));
                        }
                        Err(e) => self.message(Err::<String, _>(e)),
                    }
                }
                let movement = ui.ctx().input_mut(|input| {
                    if input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                        Some((0, -1))
                    } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                        Some((0, 1))
                    } else if input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowUp) {
                        Some((-1, 0))
                    } else if input.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowDown)
                        || input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    {
                        Some((1, 0))
                    } else {
                        None
                    }
                });
                if let Some((dr, dc)) = movement
                    && let Some(row) = ids.iter().position(|n| n == &id)
                {
                    let fields = [1, 3, 5];
                    let current = fields.iter().position(|c| *c == col).unwrap_or(0);
                    let flat = (row * 3 + current) as isize + dr * 3 + dc;
                    if flat >= 0 && flat < (ids.len() * 3) as isize {
                        self.v4.table.pending_focus =
                            Some((ids[flat as usize / 3].clone(), fields[flat as usize % 3]));
                    }
                }
            }
        }
        let pending_row = self.v4.table.pending_focus.as_ref().and_then(|(id, _)| {
            self.v4
                .table
                .rows
                .iter()
                .position(|r| matches!(r,Row::Node(i) if self.project.nodes[*i].id==*id))
        });
        let mut jump = None;
        let mut collapse = None;
        let mut widths = self.v5.layout.columns;
        let path = self.path.clone();
        egui::ScrollArea::horizontal()
            .id_salt("table_horizontal")
            .show(ui, |ui| {
                ui.set_min_width(widths.iter().sum::<f32>() + 80.);
                ui.horizontal(|ui| {
                    for (i, label) in [
                        "选",
                        "标识 / 章节",
                        "类型",
                        "场景标题",
                        "发言人",
                        "正文",
                        "背景",
                        "下一节点",
                        "定位",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.add_sized(
                            [widths[i], 28.],
                            egui::Label::new(RichText::new(*label).strong().color(MUTED)),
                        );
                        if i < 8 {
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(4., 28.), Sense::drag());
                            ui.painter().vline(
                                rect.center().x,
                                rect.y_range(),
                                Stroke::new(1.0_f32, FIELD),
                            );
                            let response =
                                response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                            if response.dragged() {
                                widths[i] = (widths[i] + response.drag_delta().x)
                                    .clamp(if i == 0 { 28. } else { 65. }, 800.);
                            }
                        }
                    }
                });
                ui.separator();
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("script_table")
                    .max_height(ui.available_height().max(160.));
                if let Some(row) = pending_row
                    && !self.v4.table.visible.contains(&row)
                {
                    scroll = scroll.vertical_scroll_offset(row as f32 * 110.);
                }
                scroll.show_rows(ui, 100., self.v4.table.rows.len(), |ui, range| {
                    self.v4.table.visible = range.clone();
                    for row in range {
                        let entry = self.v4.table.rows[row].clone();
                        ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), 100.),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| match entry {
                                Row::Chapter(chapter, count) => {
                                    if ui
                                        .button(format!(
                                            "{}  {} · {count} 段",
                                            if self.v4.table.collapsed.contains(&chapter) {
                                                "＋"
                                            } else {
                                                "−"
                                            },
                                            chapter
                                        ))
                                        .clicked()
                                    {
                                        collapse = Some(chapter);
                                    }
                                }
                                Row::Node(index) => {
                                    let node = &mut self.project.nodes[index];
                                    ui.push_id(&node.id, |ui| {
                                        egui::Frame::new()
                                            .fill(if self.v4.table.selected.contains(&node.id) {
                                                Color32::from_rgb(55, 49, 77)
                                            } else {
                                                PANEL
                                            })
                                            .inner_margin(2)
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    let mut selected =
                                                        self.v4.table.selected.contains(&node.id);
                                                    if ui
                                                        .add_sized(
                                                            [widths[0], 78.],
                                                            egui::Checkbox::without_text(
                                                                &mut selected,
                                                            ),
                                                        )
                                                        .changed()
                                                    {
                                                        if selected {
                                                            self.v4
                                                                .table
                                                                .selected
                                                                .insert(node.id.clone());
                                                        } else {
                                                            self.v4.table.selected.remove(&node.id);
                                                        }
                                                    }
                                                    ui.add_space(4.);
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(widths[1], 78.),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            ui.label(
                                                                RichText::new(&node.id)
                                                                    .small()
                                                                    .color(ACCENT),
                                                            );
                                                            text_cell(
                                                                ui,
                                                                &path,
                                                                &node.id,
                                                                1,
                                                                &mut node.chapter,
                                                                Vec2::new(widths[1], 34.),
                                                                false,
                                                                &mut self.v4.table,
                                                            );
                                                        },
                                                    );
                                                    ui.add_space(4.);
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(widths[2], 78.),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            egui::ComboBox::from_id_salt("kind")
                                                                .width((widths[2] - 10.).max(55.))
                                                                .selected_text(node.kind.label())
                                                                .show_ui(ui, |ui| {
                                                                    for kind in [
                                                                        NodeKind::Dialogue,
                                                                        NodeKind::Choice,
                                                                        NodeKind::Ending,
                                                                    ] {
                                                                        ui.selectable_value(
                                                                            &mut node.kind,
                                                                            kind,
                                                                            kind.label(),
                                                                        );
                                                                    }
                                                                });
                                                        },
                                                    );
                                                    ui.add_space(4.);
                                                    text_cell(
                                                        ui,
                                                        &path,
                                                        &node.id,
                                                        3,
                                                        &mut node.title,
                                                        Vec2::new(widths[3], 68.),
                                                        false,
                                                        &mut self.v4.table,
                                                    );
                                                    ui.add_space(4.);
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(widths[4], 78.),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            let old = node.character.clone();
                                                            sized_optional(
                                                                ui,
                                                                "speaker",
                                                                &mut node.character,
                                                                chars,
                                                                "旁白",
                                                                widths[4],
                                                            );
                                                            if old != node.character {
                                                                node.expression = None;
                                                            }
                                                        },
                                                    );
                                                    ui.add_space(4.);
                                                    text_cell(
                                                        ui,
                                                        &path,
                                                        &node.id,
                                                        5,
                                                        &mut node.text,
                                                        Vec2::new(widths[5], 68.),
                                                        true,
                                                        &mut self.v4.table,
                                                    );
                                                    ui.add_space(4.);
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(widths[6], 78.),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            sized_optional(
                                                                ui,
                                                                "background",
                                                                &mut node.background,
                                                                bgs,
                                                                "纯色舞台",
                                                                widths[6],
                                                            );
                                                        },
                                                    );
                                                    ui.add_space(4.);
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(widths[7], 78.),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            if node.kind == NodeKind::Dialogue {
                                                                let mut value = node.next.clone();
                                                                sized_optional(
                                                                    ui,
                                                                    "next",
                                                                    &mut value,
                                                                    targets,
                                                                    "无跳转",
                                                                    widths[7],
                                                                );
                                                                node.next = value;
                                                            } else {
                                                                ui.label(
                                                                    if node.kind == NodeKind::Choice
                                                                    {
                                                                        "在场景中编辑选项"
                                                                    } else {
                                                                        "终点"
                                                                    },
                                                                );
                                                            }
                                                        },
                                                    );
                                                    ui.add_space(4.);
                                                    if ui
                                                        .add_sized(
                                                            [widths[8], 36.],
                                                            egui::Button::new("场景"),
                                                        )
                                                        .clicked()
                                                    {
                                                        jump = Some(index);
                                                    }
                                                });
                                            });
                                    });
                                }
                            },
                        );
                    }
                });
            });
        self.v5.layout.columns = widths;
        if let Some(chapter) = collapse {
            if !self.v4.table.collapsed.remove(&chapter) {
                self.v4.table.collapsed.insert(chapter);
            }
            self.v4.table.revision = u64::MAX;
        }
        jump
    }
    pub(super) fn csv_preview(&mut self, ctx: &egui::Context) {
        if self.v4.table.preview.is_none() {
            return;
        }
        let mut apply = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("csv_preview")).show(ctx, |ui| {
            ui.set_width(550.);
            ui.heading("确认 CSV 导入");
            let preview = self.v4.table.preview.as_ref().unwrap();
            ui.label(format!(
                "更新 {} 段，新增 {} 段；没有出现在 CSV 的旧节点会保留。",
                preview.changed, preview.added
            ));
            ui.label("引用与条件校验已通过。应用后可以撤销；正式工程仍需 Ctrl+S 保存。");
            egui::ScrollArea::vertical()
                .max_height(230.)
                .show(ui, |ui| {
                    for node in preview
                        .project
                        .nodes
                        .iter()
                        .filter(|n| self.project.node(&n.id) != Some(*n))
                        .take(12)
                    {
                        ui.label(format!("{} · {}", node.id, node.title));
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("应用到当前项目").clicked() {
                    apply = true;
                }
                if ui.button("取消导入").clicked() {
                    cancel = true;
                }
            });
        });
        if apply && let Some(preview) = self.v4.table.preview.take() {
            if preview.source_fingerprint != fingerprint(&self.project) {
                self.message(Err::<String, _>(anyhow::anyhow!(
                    "原工程已变化，请重新预览 CSV 后应用"
                )));
                return;
            }
            self.project = preview.project;
            self.v4.table.revision = u64::MAX;
            self.message(Ok("CSV 已应用，可撤销；请保存项目。"));
        }
        if cancel {
            self.v4.table.preview = None;
        }
    }
}
fn cell_id(path: &Path, id: &str, column: usize) -> egui::Id {
    egui::Id::new(("grid_cell", path, id, column))
}
#[allow(clippy::too_many_arguments)]
fn text_cell(
    ui: &mut egui::Ui,
    path: &Path,
    id: &str,
    column: usize,
    value: &mut String,
    size: Vec2,
    multiline: bool,
    state: &mut TableState,
) {
    let widget_id = cell_id(path, id, column);
    if state
        .pending_focus
        .as_ref()
        .is_some_and(|(node, col)| node == id && *col == column)
    {
        ui.ctx().memory_mut(|m| m.request_focus(widget_id));
        state.pending_focus = None;
    }
    let editor = if multiline {
        egui::TextEdit::multiline(value).return_key(egui::KeyboardShortcut::new(
            egui::Modifiers::SHIFT,
            egui::Key::Enter,
        ))
    } else {
        egui::TextEdit::singleline(value)
    };
    let response = ui.add_sized(size, editor.id(widget_id).char_limit(65536));
    if response.has_focus() {
        state.active = Some((id.into(), column));
    }
}
fn sized_optional(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    items: &[(String, String)],
    empty: &str,
    width: f32,
) {
    let label = value
        .as_ref()
        .and_then(|v| items.iter().find(|(k, _)| k == v))
        .map(|(_, label)| label.as_str())
        .unwrap_or(empty);
    egui::ComboBox::from_id_salt(id)
        .width((width - 10.).max(55.))
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, empty);
            egui::ScrollArea::vertical().max_height(240.).show_rows(
                ui,
                34.,
                items.len(),
                |ui, range| {
                    for i in range {
                        ui.selectable_value(value, Some(items[i].0.clone()), &items[i].1);
                    }
                },
            );
        });
}
