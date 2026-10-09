use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioPaths {
    pub pz_install_dir: String,
    pub workshop_dir: String,
    pub user_zomboid_dir: String,
    pub mod_list_ini_path: String,
    pub carrier_workshop_id: Option<String>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetingModFileRaw {
    pub mod_id: String,
    pub mod_name: String,
    pub absolute_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsConflictRaw {
    pub id: String,
    pub relative_path: String,
    pub file_type: String, // "LUA" or "SCRIPT_TXT"
    pub start_line: usize,
    pub end_line: usize,
    pub conflict_line: usize,
    pub total_file_lines: usize,
    pub base_content: String,
    pub competing_files: Vec<CompetingModFileRaw>,
    pub merged_output: String,
    pub auto_ast_output: String,
    pub is_identical_noise: bool,
    pub status: Option<String>,
    /// "Error" | "Warning" | "Info" — how much this collision actually matters.
    pub severity: String,
    /// One plain-language sentence explaining the collision. No jargon.
    pub cause: String,
}

/// Pure classifier for a VFS file collision. Total: any `(count, identical)`
/// pair yields a valid answer, with no unwrap, expect or panic.
///
/// - 0 or 1 competing mod -> nothing is actually colliding.
/// - Identical copies -> harmless duplication, load order cannot change anything.
/// - Differing copies -> only the last mod in load order is really used.
pub fn classify_vfs_conflict(competing_mod_count: usize, identical: bool) -> (&'static str, String) {
    match competing_mod_count {
        0 => (
            "Info",
            "No mod provides this file.".to_string(),
        ),
        1 => (
            "Info",
            "Only one mod provides this file.".to_string(),
        ),
        _ if identical => (
            "Info",
            format!(
                "{} mods contain an identical copy of this file, so load order cannot change \
                 behaviour.",
                competing_mod_count
            ),
        ),
        _ => (
            "Error",
            format!(
                "{} mods define different content for the same file; only the last one in load \
                 order is actually used.",
                competing_mod_count
            ),
        ),
    }
}

/// Auto-detects default Project Zomboid installation and user data paths across drives.
pub fn auto_detect_paths() -> StudioPaths {
    let user_home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("C:\\"));
    let user_zomboid = user_home.join("Zomboid");
    let mod_list_ini = user_zomboid.join("mods").join("ModListData.ini");

    let mut pz_install_dir = String::new();
    let mut workshop_dir = String::new();

    let drives = vec!["C", "D", "E", "F", "G", "H", "I", "J", "K", "Z"];

    for drive in drives {
        let base_roots = vec![
            format!("{}:\\Program Files (x86)\\Steam", drive),
            format!("{}:\\Program Files\\Steam", drive),
            format!("{}:\\Steam", drive),
            format!("{}:\\SteamLibrary", drive),
            format!("{}:\\Apps\\Steam", drive),
            format!("{}:\\Apps", drive),
            format!("{}:\\Juegos", drive),
            format!("{}:\\Juegos\\Steam", drive),
            format!("{}:\\Games", drive),
            format!("{}:\\Games\\Steam", drive),
        ];

        for root in &base_roots {
            let root_path = Path::new(root);
            if root_path.exists() {
                let workshop_p = root_path.join("steamapps").join("workshop").join("content").join("108600");
                if workshop_p.exists() && workshop_dir.is_empty() {
                    workshop_dir = workshop_p.to_string_lossy().to_string();
                }

                let pz_p = root_path.join("steamapps").join("common").join("ProjectZomboid");
                if pz_p.exists() && pz_install_dir.is_empty() {
                    pz_install_dir = pz_p.to_string_lossy().to_string();
                }
            }
        }
    }

    let user_zomboid_dir = if user_zomboid.exists() {
        user_zomboid.to_string_lossy().to_string()
    } else {
        String::new()
    };

    let mod_list_ini_path = mod_list_ini.to_string_lossy().to_string();
    let is_valid = !pz_install_dir.is_empty() && !user_zomboid_dir.is_empty();

    StudioPaths {
        pz_install_dir,
        workshop_dir,
        user_zomboid_dir,
        mod_list_ini_path,
        carrier_workshop_id: None,
        is_valid,
    }
}

/// Validates provided paths.
pub fn validate_paths(mut paths: StudioPaths) -> StudioPaths {
    let pz_ok = !paths.pz_install_dir.is_empty() && Path::new(&paths.pz_install_dir).exists();
    let user_ok = !paths.user_zomboid_dir.is_empty() && Path::new(&paths.user_zomboid_dir).exists();

    if pz_ok && (paths.workshop_dir.is_empty() || !Path::new(&paths.workshop_dir).exists()) {
        let install_path = Path::new(&paths.pz_install_dir);
        if let Some(parent) = install_path.parent().and_then(|p| p.parent()) {
            let candidate = parent.join("workshop").join("content").join("108600");
            if candidate.exists() {
                paths.workshop_dir = candidate.to_string_lossy().to_string();
            }
        }
    }

    if user_ok && paths.mod_list_ini_path.is_empty() {
        let candidate = Path::new(&paths.user_zomboid_dir).join("mods").join("ModListData.ini");
        paths.mod_list_ini_path = candidate.to_string_lossy().to_string();
    }

    paths.is_valid = pz_ok && user_ok;
    paths
}

/// Scans active mods to detect relative file path collisions.
pub fn scan_conflicts(paths: &StudioPaths) -> Vec<VfsConflictRaw> {
    let mut file_map: HashMap<String, Vec<CompetingModFileRaw>> = HashMap::new();
    let mut active_mods: Vec<String> = Vec::new();

    if let Ok(ini_data) = crate::load_order::ini_parser::read_mod_list_ini(&paths.mod_list_ini_path) {
        active_mods = ini_data
            .active_mods
            .into_iter()
            .filter(|s| !s.starts_with("Z_PZModStudio_") && s != "PZModStudioCarrier")
            .collect();
    }

    let is_mod_active = |resolved_id: &str, path: &Path| -> bool {
        if active_mods.is_empty() {
            return true;
        }

        let clean_target = resolved_id.trim().to_lowercase();
        let sanitized_target = crate::load_order::mod_info::sanitize_mod_id(&clean_target);
        let alpha_target: String = clean_target.chars().filter(|c| c.is_alphanumeric()).collect();

        for active in &active_mods {
            let clean_act = active.trim().to_lowercase();
            let sanitized_act = crate::load_order::mod_info::sanitize_mod_id(&clean_act);
            let alpha_act: String = clean_act.chars().filter(|c| c.is_alphanumeric()).collect();

            if clean_act == clean_target 
                || sanitized_act == sanitized_target 
                || (!alpha_target.is_empty() && alpha_target == alpha_act) {
                return true;
            }
        }

        // Fallback: check workshop numeric id from path
        if let Some(ws_id) = extract_mod_id_from_path(path) {
            if active_mods.iter().any(|a| a == &ws_id) {
                return true;
            }
        }

        false
    };

    let workshop_dir = Path::new(&paths.workshop_dir);
    if workshop_dir.exists() {
        for entry in WalkDir::new(workshop_dir).max_depth(8).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let path_str = path.to_string_lossy();
                if (path_str.ends_with(".lua") || path_str.ends_with(".txt")) 
                   && !path_str.contains("Z_PZModStudio_") 
                   && !path_str.contains("PZModStudioCarrier") {
                    if let Some(rel_path) = extract_relative_media_path(path) {
                        let mod_id = resolve_specific_mod_id(path)
                            .or_else(|| extract_mod_id_from_path(path))
                            .unwrap_or_else(|| "workshop_mod".to_string());

                        if is_mod_active(&mod_id, path) {
                            let mod_name = resolve_specific_mod_name(path, &mod_id);
                            if let Ok(content) = fs::read_to_string(path) {
                                file_map.entry(rel_path).or_default().push(CompetingModFileRaw {
                                    mod_id,
                                    mod_name,
                                    absolute_path: path_str.to_string(),
                                    content,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let user_mods_dir = Path::new(&paths.user_zomboid_dir).join("mods");
    if user_mods_dir.exists() {
        for entry in WalkDir::new(&user_mods_dir).max_depth(8).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let path_str = path.to_string_lossy();
                if (path_str.ends_with(".lua") || path_str.ends_with(".txt")) 
                   && !path_str.contains("Z_PZModStudio_") 
                   && !path_str.contains("PZModStudioCarrier") {
                    if let Some(rel_path) = extract_relative_media_path(path) {
                        let mod_id = resolve_specific_mod_id(path)
                            .or_else(|| extract_local_mod_id(path))
                            .unwrap_or_else(|| "local_mod".to_string());

                        if is_mod_active(&mod_id, path) {
                            let mod_name = resolve_specific_mod_name(path, &mod_id);
                            if let Ok(content) = fs::read_to_string(path) {
                                file_map.entry(rel_path).or_default().push(CompetingModFileRaw {
                                    mod_id,
                                    mod_name,
                                    absolute_path: path_str.to_string(),
                                    content,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let pz_base_dir = Path::new(&paths.pz_install_dir);

    // Load any saved draft resolutions for active packages
    let user_dirs = crate::load_order::mod_info::get_all_user_zomboid_dirs(&paths.user_zomboid_dir);
    let mut draft_map: HashMap<String, (String, String)> = HashMap::new();
    for u_dir in &user_dirs {
        let mods_dir = u_dir.join("mods");
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().unwrap_or_default().to_string_lossy();
                    if name.starts_with("Z_PZModStudio_") {
                        let draft_file = p.join("draft_resolutions.json");
                        if draft_file.exists() {
                            if let Ok(content) = fs::read_to_string(&draft_file) {
                                if let Ok(items) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&content) {
                                    for (k, v) in items {
                                        if let (Some(c), Some(st)) = (v["resolved_content"].as_str(), v["status"].as_str()) {
                                            draft_map.insert(k, (c.to_string(), st.to_string()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut conflicts = Vec::new();
    let mut id_counter = 1;

    for (rel_path, mut competing_files) in file_map {
        // In Build 42, media/registries.lua is an additive registration file loaded natively by the game engine from all active mods.
        // It does not clobber other mods and re-registering causes duplicate null returns in Java.
        if rel_path.ends_with("registries.lua") || rel_path.ends_with("registries.LUA") {
            continue;
        }

        if competing_files.len() > 1 {
            // Prioritize Build 42 (42/) files over legacy B41 fallback copies for the same mod
            competing_files.sort_by_key(|f| {
                if f.absolute_path.contains("/42/") || f.absolute_path.contains("\\42\\") {
                    0
                } else {
                    1
                }
            });

            let mut unique_mods = Vec::new();
            let mut seen_mods = HashSet::new();

            for file in competing_files.drain(..) {
                let norm_mod_id = file.mod_id.to_lowercase().replace('-', "_").replace(' ', "");
                if !seen_mods.contains(&norm_mod_id) {
                    seen_mods.insert(norm_mod_id);
                    unique_mods.push(file);
                }
            }

            if unique_mods.len() > 1 {
                let vanilla_file_path = if rel_path.starts_with("media/") || rel_path.starts_with("media\\") {
                    pz_base_dir.join(&rel_path)
                } else {
                    pz_base_dir.join("media").join(&rel_path)
                };
                let base_content = if vanilla_file_path.exists() {
                    fs::read_to_string(&vanilla_file_path).unwrap_or_else(|_| "-- vanilla file unreadable".to_string())
                } else {
                    "-- vanilla file not present for this path".to_string()
                };

                let file_type = if rel_path.ends_with(".lua") { "LUA" } else { "SCRIPT_TXT" };
                let total_file_lines = base_content.lines().count().max(1);
                let conflict_line = find_first_differing_line(&base_content, &unique_mods);

                let is_identical_noise = if unique_mods.len() > 1 {
                    let first = unique_mods[0].content.trim();
                    unique_mods[1..].iter().all(|f| f.content.trim() == first)
                } else {
                    true
                };

                let auto_ast_output = crate::diff_engine::lua::combine_n_way_lua_or_script(&rel_path, &base_content, &unique_mods);
                let (merged_output, status) = if let Some((saved_content, saved_status)) = draft_map.get(&rel_path) {
                    (saved_content.clone(), Some(saved_status.clone()))
                } else {
                    (auto_ast_output.clone(), None)
                };

                let (severity, cause) =
                    classify_vfs_conflict(unique_mods.len(), is_identical_noise);

                conflicts.push(VfsConflictRaw {
                    id: format!("conflict_{}", id_counter),
                    relative_path: rel_path,
                    file_type: file_type.to_string(),
                    start_line: 1,
                    end_line: total_file_lines,
                    conflict_line,
                    total_file_lines,
                    base_content,
                    competing_files: unique_mods,
                    merged_output,
                    auto_ast_output,
                    is_identical_noise,
                    status,
                    severity: severity.to_string(),
                    cause,
                });

                id_counter += 1;
            }
        }
    }

    conflicts
}

/// Finds the EXACT 1-based line index where content actually differs between Vanilla and competing mods.
fn find_first_differing_line(base: &str, files: &[CompetingModFileRaw]) -> usize {
    let base_lines: Vec<&str> = base.lines().collect();
    let file_lines_list: Vec<Vec<&str>> = files.iter().map(|f| f.content.lines().collect()).collect();

    let max_lines = base_lines.len().max(
        file_lines_list.iter().map(|l| l.len()).max().unwrap_or(0)
    );

    for idx in 0..max_lines {
        let base_line = base_lines.get(idx).copied().unwrap_or("");
        
        for file_lines in &file_lines_list {
            let mod_line = file_lines.get(idx).copied().unwrap_or("");
            if base_line != mod_line {
                return idx + 1; // Return exact 1-based line number of difference
            }
        }
    }

    1
}

fn resolve_specific_mod_id(file_path: &Path) -> Option<String> {
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let info_path = dir.join("mod.info");
        if info_path.exists() {
            if let Ok(content) = fs::read_to_string(&info_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("id=") {
                        let id_val = trimmed[3..].trim().to_string();
                        if !id_val.is_empty() {
                            return Some(id_val);
                        }
                    }
                }
            }
            break;
        }
        current = dir.parent();
    }
    None
}

fn resolve_specific_mod_name(file_path: &Path, fallback_mod_id: &str) -> String {
    let mut current = file_path.parent();
    while let Some(dir) = current {
        let info_path = dir.join("mod.info");
        if info_path.exists() {
            if let Ok(content) = fs::read_to_string(&info_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("name=") {
                        let name_val = trimmed[5..].trim().to_string();
                        if !name_val.is_empty() {
                            return name_val;
                        }
                    }
                }
            }
            break;
        }
        current = dir.parent();
    }
    fallback_mod_id.to_string()
}

fn extract_relative_media_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy().replace('\\', "/");
    if let Some(idx) = path_str.find("/media/") {
        let rel = path_str[idx + 1..].to_string();
        if rel == "media/sandbox-options.txt" || rel.ends_with("/sandbox-options.txt") || rel == "media/mod.info" {
            return None;
        }
        return Some(rel);
    }
    None
}

fn extract_mod_id_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find("108600\\") {
        let rest = &path_str[idx + 7..];
        if let Some(end_idx) = rest.find('\\') {
            return Some(rest[..end_idx].to_string());
        }
    }
    None
}

fn extract_local_mod_id(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find("mods\\") {
        let rest = &path_str[idx + 5..];
        if let Some(end_idx) = rest.find('\\') {
            return Some(rest[..end_idx].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::classify_vfs_conflict;

    #[test]
    fn classify_no_mods_is_info_and_total() {
        let (severity, cause) = classify_vfs_conflict(0, false);
        assert_eq!(severity, "Info");
        assert!(!cause.is_empty());
    }

    #[test]
    fn classify_no_mods_identical_is_still_info() {
        let (severity, _cause) = classify_vfs_conflict(0, true);
        assert_eq!(severity, "Info");
    }

    #[test]
    fn classify_single_mod_is_info_regardless_of_identical() {
        let (severity, cause) = classify_vfs_conflict(1, false);
        assert_eq!(severity, "Info");
        assert!(cause.contains("Only one mod"));
        assert!(cause.to_lowercase().contains("this file"));
    }

    #[test]
    fn classify_single_mod_identical_is_info() {
        let (severity, cause) = classify_vfs_conflict(1, true);
        assert_eq!(severity, "Info");
        assert!(cause.contains("Only one mod"));
    }

    #[test]
    fn classify_two_identical_mods_is_info() {
        let (severity, cause) = classify_vfs_conflict(2, true);
        assert_eq!(severity, "Info");
        assert!(cause.contains("2 mods"));
        assert!(cause.contains("identical"));
        assert!(cause.contains("load order cannot change"));
    }

    #[test]
    fn classify_two_differing_mods_is_error() {
        let (severity, cause) = classify_vfs_conflict(2, false);
        assert_eq!(severity, "Error");
        assert!(cause.contains("2 mods"));
        assert!(cause.contains("different content"));
        assert!(cause.contains("last one in load order"));
    }

    #[test]
    fn classify_three_identical_mods_is_info_and_counts_them() {
        let (severity, cause) = classify_vfs_conflict(3, true);
        assert_eq!(severity, "Info");
        assert!(cause.contains("3 mods"));
        assert!(!cause.contains("2 mods"));
    }

    #[test]
    fn classify_five_differing_mods_is_error_and_counts_them() {
        let (severity, cause) = classify_vfs_conflict(5, false);
        assert_eq!(severity, "Error");
        assert!(cause.contains("5 mods"));
        assert!(cause.contains("different content"));
    }

    #[test]
    fn classify_never_panics_across_a_wide_input_range() {
        for n in 0..64usize {
            for identical in [false, true] {
                let (severity, cause) = classify_vfs_conflict(n, identical);
                assert!(severity == "Info" || severity == "Error" || severity == "Warning");
                assert!(!cause.is_empty());
                assert!(!cause.ends_with(' '));
            }
        }
    }
}
