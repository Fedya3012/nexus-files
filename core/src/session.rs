//! Session restore (tabs + panes) and user settings.

use serde::{Deserialize, Serialize};

use crate::model::SortOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum ViewMode {
    Details,
    List,
    Icons,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct PaneState {
    pub path: String,
    pub view_mode: ViewMode,
    pub sort: SortOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct TabState {
    pub left: PaneState,
    /// `Some` when the tab is split into two panes.
    pub right: Option<PaneState>,
    pub right_is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct SessionState {
    pub tabs: Vec<TabState>,
    pub active_tab: u32,
    pub sidebar_visible: bool,
    pub inspector_visible: bool,
    pub sidebar_width: f64,
    pub inspector_width: f64,
    pub window: Option<WindowPlacement>,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState {
            tabs: vec![],
            active_tab: 0,
            sidebar_visible: true,
            inspector_visible: true,
            sidebar_width: 240.0,
            inspector_width: 360.0,
            window: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub show_hidden: bool,
    pub confirm_trash: bool,
    pub restore_session: bool,
    pub default_view: ViewMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: ThemePreference::System, show_hidden: false, confirm_trash: false, restore_session: true, default_view: ViewMode::Details }
    }
}
