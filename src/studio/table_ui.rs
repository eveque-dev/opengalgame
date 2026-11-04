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
}
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
        let mut jump = None;
        let mut collapse = None;
        egui::ScrollArea::horizontal().id_salt("table_horizontal").show(ui,|ui|{
            ui.set_min_width(1460.);ui.horizontal(|ui|{for(width,label)in[(32.,"选"),(165.,"标识 / 章节"),(95.,"类型"),(150.,"场景标题"),(170.,"发言人"),(350.,"正文"),(170.,"背景"),(190.,"下一节点"),(65.,"定位")]{ui.add_sized([width,28.],egui::Label::new(RichText::new(label).strong().color(MUTED)));}});ui.separator();
            egui::ScrollArea::vertical().id_salt("script_table").max_height(ui.available_height().max(160.)).show_rows(ui,100.,self.v4.table.rows.len(),|ui,range|{
                for row in range{match &self.v4.table.rows[row]{Row::Chapter(chapter,count)=>{ui.horizontal(|ui|{if ui.button(format!("{}  {}  ·  {count} 段",if self.v4.table.collapsed.contains(chapter){"＋"}else{"−"},chapter)).clicked(){collapse=Some(chapter.clone());}});ui.add_space(55.);},Row::Node(index)=>{let node=&mut self.project.nodes[*index];ui.push_id(&node.id,|ui|{egui::Frame::new().fill(if self.v4.table.selected.contains(&node.id){Color32::from_rgb(55,49,77)}else{PANEL}).inner_margin(2).show(ui,|ui|{ui.horizontal(|ui|{
                    let mut selected=self.v4.table.selected.contains(&node.id);if ui.add_sized([32.,68.],egui::Checkbox::without_text(&mut selected)).changed(){if selected{self.v4.table.selected.insert(node.id.clone());}else{self.v4.table.selected.remove(&node.id);}}
                    ui.allocate_ui_with_layout(Vec2::new(165.,78.),egui::Layout::top_down(egui::Align::Min),|ui|{ui.label(RichText::new(&node.id).small().color(ACCENT));ui.add_sized([160.,34.],egui::TextEdit::singleline(&mut node.chapter));});
                    ui.allocate_ui_with_layout(Vec2::new(95.,78.),egui::Layout::top_down(egui::Align::Min),|ui|{egui::ComboBox::from_id_salt("kind").width(85.).selected_text(node.kind.label()).show_ui(ui,|ui|{for kind in[NodeKind::Dialogue,NodeKind::Choice,NodeKind::Ending]{ui.selectable_value(&mut node.kind,kind,kind.label());}});});
                    ui.add_sized([150.,68.],egui::TextEdit::multiline(&mut node.title).desired_rows(2));
                    ui.allocate_ui_with_layout(Vec2::new(170.,78.),egui::Layout::top_down(egui::Align::Min),|ui|{let old=node.character.clone();compact_optional(ui,"speaker",&mut node.character,&chars,"旁白");if old!=node.character{node.expression=None;}});
                    ui.add_sized([350.,68.],egui::TextEdit::multiline(&mut node.text).desired_rows(2));
                    ui.allocate_ui_with_layout(Vec2::new(170.,78.),egui::Layout::top_down(egui::Align::Min),|ui|{compact_optional(ui,"background",&mut node.background,&bgs,"纯色舞台");});
                    ui.allocate_ui_with_layout(Vec2::new(190.,78.),egui::Layout::top_down(egui::Align::Min),|ui|{if node.kind==NodeKind::Dialogue{let mut next=node.next.clone().unwrap_or_default();egui::ComboBox::from_id_salt("next").width(180.).selected_text(&next).show_ui(ui,|ui|{egui::ScrollArea::vertical().max_height(220.).show_rows(ui,34.,targets.len(),|ui,range|{for i in range{ui.selectable_value(&mut next,targets[i].0.clone(),&targets[i].1);}});});node.next=(!next.is_empty()).then_some(next);}else{ui.label(if node.kind==NodeKind::Choice{"在场景中编辑选项"}else{"终点"});}});
                    if ui.add_sized([65.,36.],egui::Button::new("场景")).clicked(){jump=Some(*index);}
                });});});}}
                }
            });
        });
        if let Some(chapter) = collapse {
            if !self.v4.table.collapsed.remove(&chapter) {
                self.v4.table.collapsed.insert(chapter);
            }
            self.v4.table.revision = u64::MAX;
        }
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
fn compact_optional(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    items: &[(String, String)],
    empty: &str,
) {
    let label = value
        .as_ref()
        .and_then(|v| items.iter().find(|(k, _)| k == v))
        .map(|(_, v)| v.as_str())
        .unwrap_or(empty);
    egui::ComboBox::from_id_salt(id)
        .width(160.)
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, empty);
            for (key, name) in items {
                ui.selectable_value(value, Some(key.clone()), name);
            }
        });
}
