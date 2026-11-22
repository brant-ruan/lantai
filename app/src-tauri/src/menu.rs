//! Native menu bar: builds the app's menus (standard macOS items plus view switching,
//! settings and update checks) and forwards clicks on our own items to the frontend as
//! `menu-action` events.
//! Standard items (Edit, Hide, Minimize, ...) are handled by the OS and never reach us.

use serde::{Deserialize, Serialize};
use tauri::menu::{
    Menu, MenuEvent, MenuItem, Submenu, SubmenuBuilder, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Runtime};

/// Event the frontend listens to for menu actions it has to carry out.
pub const MENU_ACTION_EVENT: &str = "menu-action";

const ID_ABOUT: &str = "app.about";
const ID_CHECK_UPDATES: &str = "app.check-updates";
const ID_SETTINGS: &str = "app.settings";
const ID_SHOW_MAIN_WINDOW: &str = "window.show-main";

/// View menu entries: (menu id, view mode sent to the frontend, accelerator).
const VIEW_ITEMS: [(&str, &str, &str); 6] = [
    ("view.daily", "daily", "CmdOrCtrl+1"),
    ("view.weekly", "weekly", "CmdOrCtrl+2"),
    ("view.monthly", "monthly", "CmdOrCtrl+3"),
    ("view.projects", "projects", "CmdOrCtrl+4"),
    ("view.stats", "stats", "CmdOrCtrl+5"),
    ("view.search", "search", "CmdOrCtrl+F"),
];

/// Payload of `menu-action`; the frontend switches on `kind`.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum MenuAction {
    View { mode: &'static str },
    Settings,
    About,
    CheckUpdates,
}

/// Every label of the menu bar. The frontend sends them in the interface language (its
/// language files are the only place translations live); until then, and for any label
/// it leaves out, the English one is used. `{name}` stands for the app's name.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MenuLabels {
    pub about: String,
    pub check_updates: String,
    pub settings: String,
    pub services: String,
    pub hide: String,
    pub hide_others: String,
    pub show_all: String,
    pub quit: String,
    pub edit: String,
    pub undo: String,
    pub redo: String,
    pub cut: String,
    pub copy: String,
    pub paste: String,
    pub select_all: String,
    pub view: String,
    pub daily: String,
    pub weekly: String,
    pub monthly: String,
    pub projects: String,
    pub statistics: String,
    pub search: String,
    pub full_screen: String,
    pub window: String,
    pub minimize: String,
    pub zoom: String,
    pub show_main_window: String,
    pub bring_all_to_front: String,
    pub close_window: String,
    pub help: String,
}

impl Default for MenuLabels {
    fn default() -> Self {
        let s = |t: &str| t.to_string();
        Self {
            about: s("About {name}"),
            check_updates: s("Check for Updates\u{2026}"),
            settings: s("Settings\u{2026}"),
            services: s("Services"),
            hide: s("Hide {name}"),
            hide_others: s("Hide Others"),
            show_all: s("Show All"),
            quit: s("Quit {name}"),
            edit: s("Edit"),
            undo: s("Undo"),
            redo: s("Redo"),
            cut: s("Cut"),
            copy: s("Copy"),
            paste: s("Paste"),
            select_all: s("Select All"),
            view: s("View"),
            daily: s("Daily"),
            weekly: s("Weekly"),
            monthly: s("Monthly"),
            projects: s("Projects"),
            statistics: s("Statistics"),
            search: s("Search"),
            full_screen: s("Enter Full Screen"),
            window: s("Window"),
            minimize: s("Minimize"),
            zoom: s("Zoom"),
            show_main_window: s("Show Main Window"),
            bring_all_to_front: s("Bring All to Front"),
            close_window: s("Close Window"),
            help: s("Help"),
        }
    }
}

/// The menu bar with English labels, installed at startup.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    build_with(app, &MenuLabels::default())
}

/// Build the whole menu bar. Setting a menu replaces Tauri's default one, so every standard
/// item the default had (Edit commands, Hide, Minimize, Full Screen, ...) is listed here again.
pub fn build_with<R: Runtime>(app: &AppHandle<R>, l: &MenuLabels) -> tauri::Result<Menu<R>> {
    let name = app_display_name(app);
    let named = |label: &str| label.replace("{name}", &name);

    let app_menu = SubmenuBuilder::new(app, &name)
        .item(&MenuItem::with_id(
            app,
            ID_ABOUT,
            named(&l.about),
            true,
            None::<&str>,
        )?)
        .item(&MenuItem::with_id(
            app,
            ID_CHECK_UPDATES,
            &l.check_updates,
            true,
            None::<&str>,
        )?)
        .separator()
        .item(&MenuItem::with_id(
            app,
            ID_SETTINGS,
            &l.settings,
            true,
            Some("CmdOrCtrl+,"),
        )?)
        .separator()
        .services_with_text(&l.services)
        .separator()
        .hide_with_text(named(&l.hide))
        .hide_others_with_text(&l.hide_others)
        .show_all_with_text(&l.show_all)
        .separator()
        .quit_with_text(named(&l.quit))
        .build()?;

    // Without these, Cmd+C / Cmd+V / Cmd+Z stop working in text fields on macOS.
    let edit_menu = SubmenuBuilder::new(app, &l.edit)
        .undo_with_text(&l.undo)
        .redo_with_text(&l.redo)
        .separator()
        .cut_with_text(&l.cut)
        .copy_with_text(&l.copy)
        .paste_with_text(&l.paste)
        .select_all_with_text(&l.select_all)
        .build()?;

    let view_labels = [
        &l.daily,
        &l.weekly,
        &l.monthly,
        &l.projects,
        &l.statistics,
        &l.search,
    ];
    let mut view_builder = SubmenuBuilder::new(app, &l.view);
    for ((id, _, accel), label) in VIEW_ITEMS.iter().zip(view_labels) {
        view_builder = view_builder.item(&MenuItem::with_id(app, *id, label, true, Some(*accel))?);
    }
    let view_menu = view_builder
        .separator()
        .fullscreen_with_text(&l.full_screen)
        .build()?;

    // The fixed id makes macOS treat it as the Window menu (window list, tiling items).
    let window_menu = SubmenuBuilder::with_id(app, WINDOW_SUBMENU_ID, &l.window)
        .minimize_with_text(&l.minimize)
        .maximize_with_text(&l.zoom)
        .separator()
        .item(&MenuItem::with_id(
            app,
            ID_SHOW_MAIN_WINDOW,
            &l.show_main_window,
            true,
            Some("CmdOrCtrl+0"),
        )?)
        .bring_all_to_front_with_text(&l.bring_all_to_front)
        .separator()
        .close_window_with_text(&l.close_window)
        .build()?;

    // Left empty on purpose: macOS adds its menu search field to the Help menu.
    let help_menu = Submenu::with_id(app, HELP_SUBMENU_ID, &l.help, true)?;

    Menu::with_items(
        app,
        &[&app_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
    )
}

/// Rebuild the menu bar with labels in the interface language.
#[tauri::command]
pub fn set_menu_labels(app: AppHandle, labels: MenuLabels) -> Result<(), String> {
    let menu = build_with(&app, &labels).map_err(|e| e.to_string())?;
    app.set_menu(menu).map_err(|e| e.to_string())?;
    Ok(())
}

/// Handle a click on one of our own items. Every action brings the main window back first,
/// since the menu bar stays usable after Cmd+W has hidden it.
pub fn handle_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    let action = match id {
        ID_SHOW_MAIN_WINDOW => None,
        ID_SETTINGS => Some(MenuAction::Settings),
        ID_ABOUT => Some(MenuAction::About),
        ID_CHECK_UPDATES => Some(MenuAction::CheckUpdates),
        _ => match VIEW_ITEMS.iter().find(|(item_id, ..)| *item_id == id) {
            Some((_, mode, ..)) => Some(MenuAction::View { mode }),
            None => return,
        },
    };
    crate::focus_main_window(app);
    if let Some(action) = action {
        let _ = app.emit(MENU_ACTION_EVENT, action);
    }
}

/// The name macOS shows for the app (localized, e.g. "兰台" on a Chinese system), so our
/// "About …" item matches the system's "Hide …" and "Quit …" items.
#[cfg(target_os = "macos")]
fn app_display_name<R: Runtime>(app: &AppHandle<R>) -> String {
    objc2_app_kit::NSRunningApplication::currentApplication()
        .localizedName()
        .map(|n| n.to_string())
        .unwrap_or_else(|| app.package_info().name.clone())
}

#[cfg(not(target_os = "macos"))]
fn app_display_name<R: Runtime>(app: &AppHandle<R>) -> String {
    app.package_info().name.clone()
}
