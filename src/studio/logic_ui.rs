use super::*;
use opengalgame::logic::{Compare, Condition, Effect, Value, ValueKind};
pub(super) fn value_editor(ui: &mut egui::Ui, value: &mut Value) {
    match value {
        Value::Bool(v) => {
            ui.checkbox(v, "真");
        }
        Value::Number(v) => {
            integer_editor(ui, v);
        }
        Value::Text(v) => {
            ui.add(
                egui::TextEdit::singleline(v)
                    .desired_width(180.)
                    .char_limit(16384),
            );
        }
    }
}
fn integer_editor(ui: &mut egui::Ui, value: &mut i64) {
    let id = ui.make_persistent_id("integer_value");
    let focused = ui.memory(|m| m.has_focus(id));
    let mut text = if focused {
        ui.data_mut(|d| {
            d.get_temp::<String>(id)
                .unwrap_or_else(|| value.to_string())
        })
    } else {
        value.to_string()
    };
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(120.),
    );
    if response.changed()
        && let Ok(parsed) = text.parse::<i64>()
    {
        *value = parsed;
    }
    if text.parse::<i64>().is_err() {
        ui.colored_label(BAD, "请输入 64 位整数，当前输入尚未应用");
    }
    ui.data_mut(|d| d.insert_temp(id, text));
}
fn variable_picker(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    key: &mut String,
    value: &mut Value,
    defs: &[(String, Value)],
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(if key.is_empty() {
            "请选择变量"
        } else {
            key.as_str()
        })
        .width(145.)
        .show_ui(ui, |ui| {
            for (name, initial) in defs {
                if ui
                    .selectable_value(
                        key,
                        name.clone(),
                        format!("{name} · {}", initial.kind().label()),
                    )
                    .changed()
                {
                    *value = initial.clone();
                }
            }
        });
}
fn initial_condition(defs: &[(String, Value)]) -> Condition {
    let (key, value) = defs
        .first()
        .cloned()
        .unwrap_or((String::new(), Value::Bool(false)));
    Condition::Compare {
        variable: key,
        op: Compare::Eq,
        value,
    }
}
fn condition_editor(
    ui: &mut egui::Ui,
    condition: &mut Condition,
    defs: &[(String, Value)],
    depth: usize,
) {
    if depth > 8 {
        ui.colored_label(BAD, "条件超过 8 层，请移除多余组");
        return;
    }
    let mut mode = match condition {
        Condition::Compare { .. } => 0,
        Condition::All { .. } => 1,
        Condition::Any { .. } => 2,
    };
    let before = mode;
    ui.horizontal(|ui| {
        ui.label("规则");
        egui::ComboBox::from_id_salt("group_kind")
            .selected_text(["单项比较", "全部满足（并且）", "任一满足（或者）"][mode])
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, 0, "单项比较");
                if depth < 8 {
                    ui.selectable_value(&mut mode, 1, "全部满足（并且）");
                    ui.selectable_value(&mut mode, 2, "任一满足（或者）");
                }
            });
    });
    if mode != before {
        let children = match condition {
            Condition::All { items } | Condition::Any { items } => items.clone(),
            _ => vec![condition.clone()],
        };
        *condition = match mode {
            0 => initial_condition(defs),
            1 => Condition::All { items: children },
            _ => Condition::Any { items: children },
        };
    }
    match condition {
        Condition::Compare {
            variable,
            op,
            value,
        } => {
            variable_picker(ui, "variable", variable, value, defs);
            let ops = if value.kind() == ValueKind::Number {
                vec![
                    Compare::Eq,
                    Compare::Ne,
                    Compare::Gt,
                    Compare::Ge,
                    Compare::Lt,
                    Compare::Le,
                ]
            } else {
                vec![Compare::Eq, Compare::Ne]
            };
            if !ops.contains(op) {
                *op = Compare::Eq;
            }
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("comparison")
                    .selected_text(op.label())
                    .width(95.)
                    .show_ui(ui, |ui| {
                        for option in ops {
                            ui.selectable_value(op, option, option.label());
                        }
                    });
                value_editor(ui, value);
            });
        }
        Condition::All { items } | Condition::Any { items } => {
            let mut remove = None;
            for (i, item) in items.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    egui::Frame::new()
                        .fill(FIELD)
                        .corner_radius(4)
                        .inner_margin(9)
                        .show(ui, |ui| {
                            condition_editor(ui, item, defs, depth + 1);
                            if ui.small_button("移除这一项").clicked() {
                                remove = Some(i);
                            }
                        });
                });
            }
            if let Some(i) = remove {
                items.remove(i);
            }
            if ui
                .add_enabled(items.len() < 16, egui::Button::new("＋ 条件项"))
                .clicked()
            {
                items.push(initial_condition(defs));
            }
        }
    }
}
pub(super) fn choice_logic(ui: &mut egui::Ui, choice: &mut Choice, defs: &[(String, Value)]) {
    ui.collapsing("复合条件与数值赋值 · 0.4", |ui| {
        if defs.is_empty() {
            ui.label("先在「变量」中创建布尔、整数或文本变量。");
            return;
        }
        let mut enabled = choice.when.is_some();
        if ui.checkbox(&mut enabled, "启用复合显示条件").changed() {
            choice.when = enabled.then(|| initial_condition(defs));
        }
        if let Some(condition) = &mut choice.when {
            condition_editor(ui, condition, defs, 0);
        }
        ui.separator();
        ui.label("选择后依次执行");
        let mut remove = None;
        for (i, effect) in choice.effects.iter_mut().enumerate() {
            ui.push_id(("effect", i), |ui| {
                egui::Frame::new()
                    .fill(FIELD)
                    .inner_margin(9)
                    .show(ui, |ui| {
                        let mut add = matches!(effect, Effect::Add { .. });
                        let mut key = match effect {
                            Effect::Set { variable, .. } | Effect::Add { variable, .. } => {
                                variable.clone()
                            }
                        };
                        let mut value = match effect {
                            Effect::Set { value, .. } => value.clone(),
                            Effect::Add { .. } => Value::Number(0),
                        };
                        variable_picker(ui, "effect_var", &mut key, &mut value, defs);
                        let is_number = defs
                            .iter()
                            .any(|(k, v)| k == &key && v.kind() == ValueKind::Number);
                        if !is_number {
                            add = false;
                        }
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut add, false, "设置为");
                            ui.add_enabled_ui(is_number, |ui| {
                                ui.selectable_value(&mut add, true, "增加 / 减少");
                            });
                        });
                        if add {
                            let mut amount = match effect {
                                Effect::Add { amount, .. } => *amount,
                                _ => 1,
                            };
                            integer_editor(ui, &mut amount);
                            ui.label(RichText::new("负数表示减少").small().color(MUTED));
                            *effect = Effect::Add {
                                variable: key,
                                amount,
                            };
                        } else {
                            let initial =
                                defs.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone());
                            if let Some(initial) = initial
                                && initial.kind() != value.kind()
                            {
                                value = initial;
                            }
                            value_editor(ui, &mut value);
                            *effect = Effect::Set {
                                variable: key,
                                value,
                            };
                        }
                        if ui.small_button("移除赋值").clicked() {
                            remove = Some(i);
                        }
                    });
            });
        }
        if let Some(i) = remove {
            choice.effects.remove(i);
        }
        if ui
            .add_enabled(
                choice.effects.len() < 32,
                egui::Button::new("＋ 选择后赋值"),
            )
            .clicked()
        {
            let (key, value) = defs[0].clone();
            choice.effects.push(Effect::Set {
                variable: key,
                value,
            });
        }
    });
}
impl Studio {
    pub(super) fn typed_variables(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("布尔、整数与文本");
        ui.label("初始值用于新游戏；已有存档保留自己的值。修改变量类型会阻止旧进度直接读取。");
        egui::ScrollArea::vertical()
            .max_height(340.)
            .id_salt("typed_variables")
            .show(ui, |ui| {
                let mut remove = None;
                for (key, value) in &mut self.project.variables {
                    ui.push_id(key, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(key).color(ACCENT));
                            ui.label(value.kind().label());
                            value_editor(ui, value);
                            if ui
                                .small_button("移除变量")
                                .on_hover_text("保留引用供检查报告定位，可撤销")
                                .clicked()
                            {
                                remove = Some(key.clone());
                            }
                        });
                    });
                }
                if let Some(key) = remove {
                    self.project.variables.remove(&key);
                }
            });
        ui.horizontal(|ui| {
            ui.label("标识");
            ui.add(
                egui::TextEdit::singleline(&mut self.v4.new_variable)
                    .hint_text("例如 affinity、player_name")
                    .desired_width(240.),
            );
            egui::ComboBox::from_id_salt("new_variable_kind")
                .selected_text(self.v4.new_kind.label())
                .show_ui(ui, |ui| {
                    for kind in [ValueKind::Number, ValueKind::Text, ValueKind::Bool] {
                        ui.selectable_value(&mut self.v4.new_kind, kind, kind.label());
                    }
                });
            if ui.button("添加故事变量").clicked() {
                let key = self.v4.new_variable.trim().to_owned();
                if key.is_empty()
                    || self.project.flags.contains_key(&key)
                    || self.project.variables.contains_key(&key)
                {
                    self.message(Err::<String, _>(anyhow::anyhow!("变量标识不能为空或重复")));
                } else {
                    self.project
                        .variables
                        .insert(key, self.v4.new_kind.initial());
                    self.project.format_version = 2;
                    self.v4.new_variable.clear();
                }
            }
        });
    }
}
