extern crate self as opengalgame;

#[cfg(windows)]
mod ai;
#[cfg(windows)]
mod ai_ui;
#[cfg(target_os = "android")]
mod android;
#[cfg(any(windows, target_os = "android"))]
mod editor_theme;
#[cfg(any(windows, target_os = "android"))]
pub mod studio;
#[cfg(windows)]
pub mod workspace;

pub mod authoring;
pub mod bundle;
pub mod catalog;
pub mod compatibility;
pub mod model;
pub mod native_icon;
pub mod nodes;
pub mod paths;
mod portrait;
pub mod project_io;
pub mod recovery;
pub mod runtime;
pub mod sound;

pub mod assets;
pub mod debugger;
pub mod logic;
pub mod production;
pub mod spreadsheet;
pub mod story_tools;
pub mod testing;
