use super::*;
pub(super) struct V4 {
    pub new_variable: String,
    pub new_kind: opengalgame::logic::ValueKind,
    pub debug: opengalgame::debugger::Debugger,
    pub table: table_ui::TableState,
}
impl Default for V4 {
    fn default() -> Self {
        Self {
            new_variable: String::new(),
            new_kind: opengalgame::logic::ValueKind::Number,
            debug: Default::default(),
            table: Default::default(),
        }
    }
}
