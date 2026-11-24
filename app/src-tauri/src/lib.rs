//! Tauri application builder: enforces a single running instance, keeps the app
//! resident in the Dock on macOS (closing the window hides it; Cmd+Q quits),
//! installs the native menu bar, wires shared state and registers command handlers.

mod app_icon;
mod cmd_agents;
mod cmd_appearance;
mod cmd_backup;
mod cmd_batch;
mod cmd_browse;
mod cmd_cli;
mod cmd_engine;
mod cmd_human;
mod cmd_language;
mod cmd_links;
mod cmd_project;
mod cmd_prompt;
mod cmd_report;
mod cmd_search;
mod cmd_self_stats;
mod cmd_stats;
mod cmd_update;
mod menu;
mod pickers;
mod shell_env;
mod state;

use state::AppState;
use tauri::{Manager, Runtime};

/// Bring the existing main window to the front: show it if hidden, restore it if minimized.
pub(crate) fn focus_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    shell_env::import_login_shell_env();
    let app_state = AppState::new().expect("failed to initialize Lantai state");
    tauri::Builder::default()
        // Must be the first plugin: a second launch exits here and focuses the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            focus_main_window(app);
        }))
        .manage(app_state)
        .menu(menu::build)
        .on_menu_event(menu::handle_event)
        .on_window_event(|window, event| {
            // macOS: Cmd+W / the close button only hide the window, so running
            // report jobs keep going and the Dock icon brings it back. Cmd+Q quits.
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (window, event);
        })
        .setup(|app| {
            let icon = app_icon::load_pref(&app.state::<AppState>().data_dir);
            app_icon::apply(app.handle(), icon, app_icon::BundleSync::IfStale);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_agents::agent_sources,
            cmd_agents::set_agent_settings,
            cmd_appearance::list_app_icons,
            cmd_appearance::set_app_icon,
            cmd_backup::export_backup,
            cmd_backup::pick_backup,
            cmd_backup::restore_backup,
            cmd_browse::scan_sessions,
            cmd_cli::cli_tool_status,
            cmd_cli::install_cli_tool,
            cmd_cli::uninstall_cli_tool,
            cmd_update::update_settings,
            cmd_update::set_check_updates,
            cmd_update::check_for_updates,
            cmd_update::open_release_page,
            cmd_browse::get_day_boundary,
            cmd_browse::set_day_boundary,
            cmd_browse::list_days,
            cmd_browse::day_workbench,
            cmd_browse::week_workbench,
            cmd_browse::month_workbench,
            cmd_browse::list_weeks,
            cmd_browse::list_months,
            cmd_browse::project_workbench,
            cmd_browse::slice_detail,
            cmd_browse::set_session_note,
            cmd_browse::set_session_ignored,
            cmd_browse::set_slice_ignore_state,
            cmd_browse::diagnostics,
            cmd_engine::model_settings,
            cmd_engine::provider_presets,
            cmd_engine::save_provider,
            cmd_engine::delete_provider,
            cmd_engine::save_model,
            cmd_engine::delete_model,
            cmd_engine::set_default_model,
            cmd_engine::list_provider_models,
            cmd_engine::test_model,
            cmd_engine::find_local_tools,
            cmd_engine::add_local_tool,
            cmd_engine::diagnose_error,
            cmd_project::list_projects_and_rules,
            cmd_project::create_project,
            cmd_project::rename_project,
            cmd_project::reorder_projects,
            cmd_project::describe_project,
            cmd_project::delete_project,
            cmd_project::set_project_archived,
            cmd_project::project_delete_impact,
            cmd_project::add_rule,
            cmd_project::delete_rule,
            cmd_project::set_rule_auto,
            cmd_project::assign_session,
            cmd_project::assign_slice,
            cmd_project::unassign_session,
            cmd_project::accept_suggestions,
            cmd_prompt::list_prompts,
            cmd_prompt::preview_prompt,
            cmd_prompt::create_prompt,
            cmd_prompt::update_prompt,
            cmd_prompt::rename_prompt,
            cmd_prompt::delete_prompt,
            cmd_prompt::set_default_prompt,
            cmd_language::get_language_settings,
            cmd_language::set_ui_language,
            cmd_language::set_report_language,
            cmd_links::open_sponsor_page,
            menu::set_menu_labels,
            cmd_report::get_daily_report_state,
            cmd_report::get_weekly_report_state,
            cmd_report::get_monthly_report_state,
            cmd_report::get_project_rollup,
            cmd_report::digest_slice_now,
            cmd_report::generate_daily_report,
            cmd_report::generate_weekly_report,
            cmd_report::generate_monthly_report,
            cmd_report::cancel_report_generation,
            cmd_report::running_report_jobs,
            cmd_report::save_report_edit,
            cmd_batch::plan_batch,
            cmd_report::set_report_status,
            cmd_batch::set_reports_status,
            cmd_report::restore_report_version,
            cmd_report::export_daily_report,
            cmd_report::pick_export_directory,
            cmd_stats::get_stats,
            cmd_human::human_create,
            cmd_human::human_delete,
            cmd_human::human_move,
            cmd_human::human_move_to,
            cmd_human::human_load,
            cmd_human::human_restore,
            cmd_human::human_update,
            cmd_links::open_note_link,
            cmd_search::search_all,
            cmd_self_stats::get_lantai_stats,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lantai")
        .run(|app, event| {
            // macOS: clicking the Dock icon while the window is hidden brings it back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                focus_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
