pub mod compat;
pub mod conflicts;
pub mod diff_engine;
pub mod fixes;
pub mod instance_manager;
pub mod load_order;
pub mod mcp;
pub mod patch_generator;
pub mod preset_manager;
pub mod sandbox;
pub mod server_manager;
pub mod vfs;

use diff_engine::lua::{three_way_merge_lua, validate_lua_syntax, LuaSyntaxCheckResult, MergeChunkResult};
use diff_engine::pz_scripts::{merge_pz_data_scripts, PzScriptMergeResult};
use instance_manager::{activate_instance, create_instance, delete_instance, list_instances, save_master_load_order, update_instance};
use load_order::ini_parser::{read_mod_list_ini, write_mod_list_ini, ModListData};
use load_order::mod_info::{scan_all_installed_mods, ModManifest};
use load_order::topological_sort::{sort_dependencies_topologically, DependencyAnalysisResult};
use patch_generator::{generate_master_patch, MasterPatchRequest, MasterPatchResult};
use preset_manager::{check_missing_preset_mods, export_preset_file, import_preset_file};
use sandbox::{launch_sandbox_and_watch, SandboxLaunchConfig};
use server_manager::{
    create_new_server_config, delete_server_config, get_connected_players,
    get_dedicated_server_logs, get_dedicated_server_status, get_server_quick_settings,
    launch_dedicated_server, list_server_configs, save_server_log_snapshot,
    save_server_quick_settings, send_server_command, stop_dedicated_server,
    sync_client_to_server,
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use vfs::{auto_detect_paths, scan_conflicts, validate_paths, StudioPaths, VfsConflictRaw};

#[tauri::command]
fn get_auto_paths() -> StudioPaths {
    auto_detect_paths()
}

#[tauri::command]
fn set_and_validate_paths(custom_paths: StudioPaths) -> StudioPaths {
    validate_paths(custom_paths)
}

#[tauri::command]
fn scan_conflicts_cmd(paths: StudioPaths) -> Vec<VfsConflictRaw> {
    scan_conflicts(&paths)
}

#[tauri::command]
fn validate_lua_syntax_cmd(code: String) -> LuaSyntaxCheckResult {
    validate_lua_syntax(&code)
}

#[tauri::command]
fn three_way_merge_lua_cmd(base: String, target_a: String, target_b: String) -> MergeChunkResult {
    three_way_merge_lua(&base, &target_a, &target_b)
}

#[tauri::command]
fn merge_pz_data_scripts_cmd(base: String, mod_a: String, mod_b: String) -> PzScriptMergeResult {
    merge_pz_data_scripts(&base, &mod_a, &mod_b)
}

#[tauri::command]
fn read_mod_list_ini_cmd(ini_path: String) -> Result<ModListData, String> {
    read_mod_list_ini(&ini_path)
}

#[tauri::command]
fn write_mod_list_ini_cmd(ini_path: String, active_mods: Vec<String>) -> Result<(), String> {
    write_mod_list_ini(&ini_path, &active_mods)
}

#[tauri::command]
fn scan_all_installed_mods_cmd(paths: StudioPaths) -> Vec<ModManifest> {
    scan_all_installed_mods(&paths)
}

#[tauri::command]
fn sort_mod_dependencies_cmd(manifests: Vec<ModManifest>) -> DependencyAnalysisResult {
    sort_dependencies_topologically(&manifests)
}

/// Runs the pure-logic conflict engine over every install found on disk,
/// WITHOUT collapsing manifests by id, so duplicate installs are visible.
#[tauri::command]
fn scan_mod_diagnostics_cmd(user_zomboid_dir: String) -> Vec<conflicts::ModDiagnostic> {
    scan_diagnostics_report(user_zomboid_dir).diagnostics
}

/// Everything [`scan_mod_diagnostics_cmd`] reports, plus what could not be
/// read while scanning and whether the compatibility catalog loaded.
///
/// `errors` is deliberately SEPARATE from `diagnostics`: a diagnostic is a
/// problem with the user's mods, an entry in `errors` is a problem with our
/// ability to see them (unreadable folder, malformed `mod.info`, corrupt saved
/// profile). Mixing them would let a broken read look like a mod conflict — or,
/// worse, let a skipped mod disappear without a trace.
#[derive(serde::Serialize)]
pub struct ModDiagnosticsReport {
    pub user_zomboid_dir: String,
    pub total_mods_scanned: usize,
    pub diagnostics: Vec<conflicts::ModDiagnostic>,
    /// One line per mod/folder that could not be read. Sorted and deduplicated.
    pub errors: Vec<String>,
    /// Where the compatibility catalog came from. `state == "unavailable"` means
    /// the UI must say "compatibility data unavailable", not "all clear".
    pub compatibility_status: compat::CompatStatus,
}

/// Shared by the Tauri command and the fix engine's plan builder.
fn resolve_paths_for(user_zomboid_dir: &str) -> StudioPaths {
    let mut paths = auto_detect_paths();
    if !user_zomboid_dir.trim().is_empty() {
        paths.user_zomboid_dir = user_zomboid_dir.to_string();
        paths.mod_list_ini_path = std::path::Path::new(user_zomboid_dir)
            .join("mods")
            .join("ModListData.ini")
            .to_string_lossy()
            .to_string();
    }
    validate_paths(paths)
}

/// Scan, analyze, and report per-mod read failures instead of swallowing them.
pub fn scan_diagnostics_report(user_zomboid_dir: String) -> ModDiagnosticsReport {
    let paths = resolve_paths_for(&user_zomboid_dir);
    let report = load_order::mod_info::scan_all_installed_mods_with_errors(&paths);
    let rules = compat::global_rules();
    let diagnostics = conflicts::analyze_with_compat(&report.all_installs, rules, None);
    ModDiagnosticsReport {
        user_zomboid_dir: paths.user_zomboid_dir,
        total_mods_scanned: report.all_installs.len(),
        diagnostics,
        errors: report.errors,
        compatibility_status: rules.status.clone(),
    }
}

/// Same data as [`scan_mod_diagnostics_cmd`] but with the errors and the
/// compatibility-data status included. Added alongside the original rather than
/// replacing it, so existing frontend code keeps working unchanged.
#[tauri::command]
fn scan_mod_diagnostics_report_cmd(user_zomboid_dir: String) -> ModDiagnosticsReport {
    scan_diagnostics_report(user_zomboid_dir)
}

#[tauri::command]
fn launch_sandbox_cmd<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    config: SandboxLaunchConfig,
) -> Result<u32, String> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    launch_sandbox_and_watch(app_handle, config, stop_signal)
}

#[tauri::command]
fn generate_master_patch_cmd(req: MasterPatchRequest) -> Result<MasterPatchResult, String> {
    generate_master_patch(req)
}

#[tauri::command]
fn pick_folder_cmd(default_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(ref path) = default_path {
        if !path.is_empty() {
            dialog = dialog.set_directory(path);
        }
    }
    dialog.pick_folder().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_save_file_cmd(
    default_name: Option<String>,
    filter_name: Option<String>,
    filter_ext: Option<String>,
) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let (Some(ref fname), Some(ref fext)) = (filter_name, filter_ext) {
        dialog = dialog.add_filter(fname, &[fext.as_str()]);
    } else {
        dialog = dialog.add_filter("PZ Mod Studio File", &["pzmerge", "pzpack", "json"]);
    }
    if let Some(name) = default_name {
        dialog = dialog.set_file_name(&name);
    }
    dialog.save_file().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_open_file_cmd(
    filter_name: Option<String>,
    filter_ext: Option<String>,
) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let (Some(ref fname), Some(ref fext)) = (filter_name, filter_ext) {
        dialog = dialog.add_filter(fname, &[fext.as_str()]);
    } else {
        dialog = dialog.add_filter("PZ Mod Studio File", &["pzmerge", "pzpack", "json"]);
    }
    dialog.pick_file().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn open_package_folder_cmd(user_zomboid_dir: String, package_folder_name: String) -> Result<bool, String> {
    let clean_pkg_name = if package_folder_name.starts_with("Z_PZModStudio_") {
        package_folder_name
    } else {
        format!("Z_PZModStudio_{}", package_folder_name)
    };

    let user_dirs = crate::load_order::mod_info::get_all_user_zomboid_dirs(&user_zomboid_dir);
    if user_dirs.is_empty() {
        return Err("User Zomboid folder not found.".to_string());
    }

    let pkg_dir = user_dirs[0].join("mods").join(&clean_pkg_name);
    let _ = std::fs::create_dir_all(&pkg_dir);

    open::that(&pkg_dir).map_err(|e| format!("Error opening physical folder: {}", e))?;
    Ok(true)
}

#[tauri::command]
fn open_logs_folder_cmd(user_zomboid_dir: String) -> Result<bool, String> {
    let user_dirs = crate::load_order::mod_info::get_all_user_zomboid_dirs(&user_zomboid_dir);
    let target = if !user_dirs.is_empty() {
        let logs_dir = user_dirs[0].join("Logs");
        if logs_dir.exists() {
            logs_dir
        } else {
            user_dirs[0].clone()
        }
    } else {
        if let Some(home) = dirs_next::home_dir() {
            let p = home.join("Zomboid").join("Logs");
            if p.exists() { p } else { home.join("Zomboid") }
        } else {
            return Err("Zomboid folder not found.".to_string());
        }
    };

    open::that(&target).map_err(|e| format!("Error opening logs folder: {}", e))?;
    Ok(true)
}

#[tauri::command]
fn list_available_log_files_cmd(user_zomboid_dir: String) -> Vec<sandbox::LogFileInfo> {
    sandbox::list_available_log_files(&user_zomboid_dir)
}

#[tauri::command]
fn read_log_file_cmd(file_path: String, max_lines: Option<usize>) -> Result<Vec<String>, String> {
    sandbox::read_log_file(&file_path, max_lines)
}

#[tauri::command]
fn open_external_url_cmd(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        cmd.args(&["/c", "start", "", &url]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Ok(_) = cmd.spawn() {
            return Ok(());
        }
    }
    open::that(&url).map_err(|e| format!("Failed to open URL in browser: {}", e))
}

#[tauri::command]
fn clean_master_patch_cmd(req: MasterPatchRequest) -> Result<bool, String> {
    patch_generator::clean_master_patch(req)
}

#[tauri::command]
fn get_master_patch_status_cmd(user_zomboid_dir: String, mod_list_ini_path: String, package_folder_name: Option<String>) -> patch_generator::MasterPatchStatusInfo {
    patch_generator::get_master_patch_status(&user_zomboid_dir, &mod_list_ini_path, package_folder_name)
}

#[tauri::command]
fn list_merged_packages_cmd(user_zomboid_dir: String, mod_list_ini_path: String) -> Vec<patch_generator::MergedPackageInfo> {
    patch_generator::list_merged_packages(&user_zomboid_dir, &mod_list_ini_path)
}

#[tauri::command]
fn create_merged_package_cmd(user_zomboid_dir: String, mod_list_ini_path: String, name: String, description: Option<String>) -> Result<patch_generator::MergedPackageInfo, String> {
    patch_generator::create_merged_package(&user_zomboid_dir, &mod_list_ini_path, &name, description.as_deref())
}

#[tauri::command]
fn rename_merged_package_cmd(user_zomboid_dir: String, mod_list_ini_path: String, old_folder: String, new_name: String, description: Option<String>) -> Result<patch_generator::MergedPackageInfo, String> {
    patch_generator::rename_merged_package(&user_zomboid_dir, &mod_list_ini_path, &old_folder, &new_name, description.as_deref())
}

#[tauri::command]
fn delete_merged_package_cmd(user_zomboid_dir: String, mod_list_ini_path: String, folder_name: String) -> Result<bool, String> {
    patch_generator::delete_merged_package(&user_zomboid_dir, &mod_list_ini_path, &folder_name)
}

#[tauri::command]
fn save_draft_resolution_cmd(
    user_zomboid_dir: String,
    package_folder_name: String,
    relative_path: String,
    resolved_content: String,
    status: String,
) -> Result<bool, String> {
    patch_generator::save_draft_resolution(
        &user_zomboid_dir,
        &package_folder_name,
        &relative_path,
        &resolved_content,
        &status,
    )
}

#[tauri::command]
fn get_draft_resolutions_cmd(
    user_zomboid_dir: String,
    package_folder_name: String,
) -> std::collections::HashMap<String, patch_generator::DraftResolutionItem> {
    patch_generator::get_draft_resolutions(&user_zomboid_dir, &package_folder_name)
}

#[tauri::command]
fn clear_draft_resolutions_cmd(
    user_zomboid_dir: String,
    package_folder_name: String,
) -> Result<bool, String> {
    patch_generator::clear_draft_resolutions(&user_zomboid_dir, &package_folder_name)
}

#[tauri::command]
fn export_merged_package_cmd(
    user_zomboid_dir: String,
    package_folder_name: String,
    target_file_path: String,
) -> Result<bool, String> {
    patch_generator::export_merged_package(&user_zomboid_dir, &package_folder_name, &target_file_path)
}

#[tauri::command]
fn import_merged_package_cmd(
    user_zomboid_dir: String,
    mod_list_ini_path: String,
    source_file_path: String,
) -> Result<patch_generator::MergedPackageInfo, String> {
    patch_generator::import_merged_package(&user_zomboid_dir, &mod_list_ini_path, &source_file_path)
}

#[tauri::command]
fn toggle_package_in_modlist_cmd(
    user_zomboid_dir: String,
    mod_list_ini_path: String,
    folder_name: String,
    enabled: bool,
) -> Result<bool, String> {
    patch_generator::toggle_package_in_modlist(&user_zomboid_dir, &mod_list_ini_path, &folder_name, enabled)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_auto_paths,
            set_and_validate_paths,
            scan_conflicts_cmd,
            validate_lua_syntax_cmd,
            three_way_merge_lua_cmd,
            merge_pz_data_scripts_cmd,
            read_mod_list_ini_cmd,
            write_mod_list_ini_cmd,
            scan_all_installed_mods_cmd,
            scan_mod_diagnostics_cmd,
            scan_mod_diagnostics_report_cmd,
            fixes::preview_mod_fixes,
            fixes::apply_mod_fix,
            fixes::list_backups,
            fixes::restore_backup,
            sort_mod_dependencies_cmd,
            launch_sandbox_cmd,
            generate_master_patch_cmd,
            clean_master_patch_cmd,
            get_master_patch_status_cmd,
            list_merged_packages_cmd,
            create_merged_package_cmd,
            rename_merged_package_cmd,
            delete_merged_package_cmd,
            toggle_package_in_modlist_cmd,
            save_draft_resolution_cmd,
            get_draft_resolutions_cmd,
            clear_draft_resolutions_cmd,
            export_merged_package_cmd,
            import_merged_package_cmd,
            open_package_folder_cmd,
            open_logs_folder_cmd,
            list_available_log_files_cmd,
            read_log_file_cmd,
            pick_folder_cmd,
            pick_save_file_cmd,
            pick_open_file_cmd,
            open_external_url_cmd,
            export_preset_file,
            import_preset_file,
            check_missing_preset_mods,
            list_server_configs,
            sync_client_to_server,
            create_new_server_config,
            delete_server_config,
            launch_dedicated_server,
            stop_dedicated_server,
            get_dedicated_server_status,
            get_dedicated_server_logs,
            save_server_log_snapshot,
            get_connected_players,
            send_server_command,
            get_server_quick_settings,
            save_server_quick_settings,
            list_instances,
            create_instance,
            activate_instance,
            delete_instance,
            update_instance,
            save_master_load_order
        ])
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let _ = stop_dedicated_server("".to_string(), None);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
