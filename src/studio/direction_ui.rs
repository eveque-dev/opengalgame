use super::*;
use opengalgame::{
    logic::{Effect, Value},
    production::*,
};
impl Studio {
    pub(super) fn direction_editor(&mut self, ui: &mut egui::Ui) {
        if self.project.nodes.is_empty() {
            return;
        }
        let id = self.project.nodes[self.selected].id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.heading("演出编排");
            ui.label(&self.project.nodes[self.selected].title);
            if ui.button("预览演出（无声）").clicked() {
                self.v5.preview = Some((id.clone(), Instant::now()));
            }
            if ui.button("停止预览").clicked() {
                self.v5.preview = None;
            }
            if ui.button("在舞台中编辑").clicked() {
                self.section = Section::Script;
            }
        });
        let preview_height = (ui.available_height() * 0.36).clamp(180., 310.);
        let width = (preview_height * 16. / 9.).min(ui.available_width());
        self.scene(ui, Vec2::new(width, preview_height), false);
        ui.separator();
        let chars: Vec<_> = self
            .project
            .characters
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();
        let backgrounds: Vec<_> = self
            .project
            .backgrounds
            .iter()
            .map(|b| (b.id.clone(), b.name.clone()))
            .collect();
        let audio: Vec<_> = self
            .project
            .audio
            .iter()
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect();
        let defs: Vec<_> = opengalgame::logic::definitions(&self.project)
            .into_iter()
            .collect();
        let project_chars = self.project.characters.clone();
        let direction = &mut self.project.nodes[self.selected].direction;
        egui::ScrollArea::vertical().id_salt("direction_controls").show(ui,|ui|{
            ui.collapsing("进入 / 离开节点时的变量操作",|ui|{
                ui.label("进入操作在首次到达时执行；回退和读档直接恢复状态。离开、选项与下一节点进入操作作为一次事务提交。");
                ui.push_id("enter",|ui|{ui.strong("进入节点后");effects_editor(ui,&mut direction.enter,&defs);});
                ui.separator();ui.push_id("exit",|ui|{ui.strong("离开节点前");effects_editor(ui,&mut direction.exit,&defs);});
            });
            ui.horizontal_wrapped(|ui|{ui.strong("逐句配音");optional_combo(ui,"line_voice",&mut direction.voice,&audio,"无配音");});
            ui.label(RichText::new("动作从上到下执行完毕后显示对白并播放配音；可拖动卡片排序，也可使用上移 / 下移。").small().color(MUTED));
            let mut reorder=None;let mut remove=None;
            let count=direction.actions.len();
            for (i,action) in direction.actions.iter_mut().enumerate(){
                let response=ui.push_id(("stage_card",&id,i),|ui|{
                    egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0_f32,FIELD)).inner_margin(12).corner_radius(8).show(ui,|ui|{
                        ui.set_min_width((ui.available_width()-8.).max(200.));
                        ui.horizontal_wrapped(|ui|{
                            let handle=ui.add(egui::Label::new("拖动排序").sense(egui::Sense::drag())).on_hover_cursor(egui::CursorIcon::Grab);
                            if handle.is_pointer_button_down_on() || handle.drag_started(){self.v5.stage_drag=Some((id.clone(),i));ui.ctx().request_repaint();}
                            ui.strong(format!("{} · {}",i+1,action.label()));
                            if ui.add_enabled(i>0,egui::Button::new("上移")).clicked(){reorder=Some((i,i-1));}
                            if ui.add_enabled(i+1<count,egui::Button::new("下移")).clicked(){reorder=Some((i,i+1));}
                            if ui.button("移除动作").clicked(){remove=Some(i);}
                        });
                        ui.horizontal_wrapped(|ui|{
                            match action {
                                StageAction::Show{character,expression,side,scale,seconds}=>{
                                    required_combo(ui,"actor",character,&chars,"选择人物");
                                    let expressions:Vec<_>=project_chars.iter().find(|c|&c.id==character).map(|c|c.expressions.keys().map(|e|(e.clone(),e.clone())).collect()).unwrap_or_default();
                                    optional_combo(ui,"expression",expression,&expressions,"默认表情");side_picker(ui,side);ui.add(egui::Slider::new(scale,0.4..=1.6).text("大小"));duration(ui,seconds);
                                }
                                StageAction::Hide{character,seconds}=>{required_combo(ui,"actor",character,&chars,"选择人物");duration(ui,seconds);}
                                StageAction::Move{character,side,scale,seconds}=>{required_combo(ui,"actor",character,&chars,"选择人物");side_picker(ui,side);ui.add(egui::Slider::new(scale,0.4..=1.6).text("大小"));duration(ui,seconds);}
                                StageAction::Expression{character,expression}=>{required_combo(ui,"actor",character,&chars,"选择人物");let expressions:Vec<_>=project_chars.iter().find(|c|&c.id==character).map(|c|c.expressions.keys().map(|e|(e.clone(),e.clone())).collect()).unwrap_or_default();optional_combo(ui,"expression",expression,&expressions,"默认表情");}
                                StageAction::Wait{seconds}=>duration(ui,seconds),
                                StageAction::Background{background,seconds}=>{required_combo(ui,"background",background,&backgrounds,"选择背景");duration(ui,seconds);}
                            }
                        });
                    });
                }).response;
                if let Some((node,from))=&self.v5.stage_drag && node==&id && ui.ctx().pointer_interact_pos().is_some_and(|p|response.rect.intersect(ui.clip_rect()).contains(p)){
                    ui.painter().rect_stroke(response.rect,8.,Stroke::new(2.0_f32,ACCENT),egui::StrokeKind::Inside);
                    if ui.input(|input|input.pointer.any_released()){reorder=Some((*from,i));}
                }
            }
            if ui.input(|input|input.pointer.any_released() || !input.pointer.primary_down()){self.v5.stage_drag=None;}
            if let Some(i)=remove{direction.actions.remove(i);}else if let Some((from,to))=reorder && from<count && to<count{let action=direction.actions.remove(from);direction.actions.insert(to,action);}
            ui.add_enabled_ui(direction.actions.len()<128,|ui|{
                ui.horizontal_wrapped(|ui|{
                    let actor=chars.first().map(|c|c.0.clone()).unwrap_or_default();
                    if ui.add_enabled(!chars.is_empty(),egui::Button::new("＋ 入场")).clicked(){direction.actions.push(StageAction::Show{character:actor.clone(),expression:None,side:PortraitSide::Left,scale:1.,seconds:0.4});}
                    if ui.add_enabled(!chars.is_empty(),egui::Button::new("＋ 移动")).clicked(){direction.actions.push(StageAction::Move{character:actor.clone(),side:PortraitSide::Right,scale:1.,seconds:0.6});}
                    if ui.add_enabled(!chars.is_empty(),egui::Button::new("＋ 表情")).clicked(){direction.actions.push(StageAction::Expression{character:actor.clone(),expression:None});}
                    if ui.add_enabled(!chars.is_empty(),egui::Button::new("＋ 退场")).clicked(){direction.actions.push(StageAction::Hide{character:actor,seconds:0.4});}
                    if ui.button("＋ 等待").clicked(){direction.actions.push(StageAction::Wait{seconds:0.5});}
                    if ui.add_enabled(!backgrounds.is_empty(),egui::Button::new("＋ 背景")).clicked(){direction.actions.push(StageAction::Background{background:backgrounds[0].0.clone(),seconds:0.5});}
                });
            });
            ui.separator();ui.heading("声音指令");
            ui.label("音乐与环境音可循环、渐变；短音效可同时叠加。配音使用上方独立通道。");
            let mut remove=None;
            for(i,cue)in direction.audio.iter_mut().enumerate(){ui.push_id(("audio_cue",i),|ui|{ui.horizontal_wrapped(|ui|{
                egui::ComboBox::from_id_salt("channel").selected_text(cue.channel.label()).show_ui(ui,|ui|{for channel in[Channel::Music,Channel::Ambience,Channel::Effects]{if ui.selectable_value(&mut cue.channel,channel,channel.label()).changed() && channel==Channel::Effects{cue.looped=false;}}});
                optional_combo(ui,"cue_audio",&mut cue.audio,&audio,"停止此通道");
                ui.add(egui::Slider::new(&mut cue.volume,0.0..=1.0).text("音量"));
                if cue.channel!=Channel::Effects{ui.checkbox(&mut cue.looped,"循环");ui.add(egui::DragValue::new(&mut cue.fade).range(0.0..=10.0).speed(0.1).suffix(" 秒渐变"));}
                if ui.button("移除声音指令").clicked(){remove=Some(i);}
            });});}
            if let Some(i)=remove{direction.audio.remove(i);}
            if ui.add_enabled(direction.audio.len()<32,egui::Button::new("＋ 声音指令")).clicked(){direction.audio.push(AudioCue::default());}
        });
    }
    pub(super) fn wide_logic(&mut self, ctx: &egui::Context) {
        if !self.v5.logic_open || self.player_only || self.runtime.is_some() {
            return;
        }
        let mut open = true;
        let defs: Vec<_> = opengalgame::logic::definitions(&self.project)
            .into_iter()
            .collect();
        let Some(node) = self.project.nodes.get_mut(self.selected) else {
            return;
        };
        egui::Window::new("条件与变量 · 宽屏编辑")
            .open(&mut open)
            .constrain_to(ctx.content_rect())
            .default_pos(Pos2::new(70., 50.))
            .default_size([850., 700.])
            .vscroll(true)
            .show(ctx, |ui| {
                ui.heading(&node.title);
                ui.push_id("node_enter", |ui| {
                    ui.collapsing("进入节点赋值", |ui| {
                        effects_editor(ui, &mut node.direction.enter, &defs)
                    });
                });
                ui.push_id("node_exit", |ui| {
                    ui.collapsing("离开节点赋值", |ui| {
                        effects_editor(ui, &mut node.direction.exit, &defs)
                    });
                });
                for (i, choice) in node.choices.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        egui::CollapsingHeader::new(format!("选项 {} · {}", i + 1, choice.text))
                            .default_open(i == self.v5.logic_choice)
                            .show(ui, |ui| {
                                logic_ui::choice_logic(ui, choice, &defs);
                            });
                    });
                }
            });
        self.v5.logic_open = open;
    }
}
fn required_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    items: &[(String, String)],
    empty: &str,
) {
    let mut selected = (!value.is_empty()).then(|| value.clone());
    optional_combo(ui, id, &mut selected, items, empty);
    *value = selected.unwrap_or_default();
}
fn side_picker(ui: &mut egui::Ui, side: &mut PortraitSide) {
    for s in [
        PortraitSide::Left,
        PortraitSide::Center,
        PortraitSide::Right,
    ] {
        ui.selectable_value(side, s, s.label());
    }
}
fn duration(ui: &mut egui::Ui, seconds: &mut f32) {
    ui.add(
        egui::DragValue::new(seconds)
            .range(0.0..=30.0)
            .speed(0.05)
            .suffix(" 秒"),
    );
}
pub(super) fn effects_editor(
    ui: &mut egui::Ui,
    effects: &mut Vec<Effect>,
    defs: &[(String, Value)],
) {
    if defs.is_empty() {
        ui.label("先在变量页创建故事变量。");
        return;
    }
    let mut remove = None;
    for (i, effect) in effects.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (key, mut value, mut add) = match &*effect {
                    Effect::Set { variable, value } => (variable.clone(), value.clone(), false),
                    Effect::Add { variable, amount } => {
                        (variable.clone(), Value::Number(*amount), true)
                    }
                };
                let mut key = key;
                egui::ComboBox::from_id_salt("effect_variable")
                    .selected_text(&key)
                    .show_ui(ui, |ui| {
                        for (name, initial) in defs {
                            if ui.selectable_value(&mut key, name.clone(), name).changed() {
                                value = initial.clone();
                                add = false;
                            }
                        }
                    });
                if matches!(
                    defs.iter().find(|(k, _)| k == &key).map(|(_, v)| v),
                    Some(Value::Number(_))
                ) {
                    ui.checkbox(&mut add, "增加 / 减少");
                }
                logic_ui::value_editor(ui, &mut value);
                *effect = if add && matches!(value, Value::Number(_)) {
                    Effect::Add {
                        variable: key,
                        amount: if let Value::Number(n) = value { n } else { 0 },
                    }
                } else {
                    Effect::Set {
                        variable: key,
                        value,
                    }
                };
                if ui.button("移除此赋值").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
    if ui
        .add_enabled(effects.len() < 64, egui::Button::new("＋ 变量操作"))
        .clicked()
    {
        effects.push(Effect::Set {
            variable: defs[0].0.clone(),
            value: defs[0].1.clone(),
        });
    }
}
