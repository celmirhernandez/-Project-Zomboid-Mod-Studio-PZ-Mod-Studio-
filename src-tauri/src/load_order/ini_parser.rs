use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use crate::load_order::mod_info::sanitize_mod_id;

#[cfg(test)]
// `ini_parser.rs` is a flat file, so a plain `mod tests;` would look for
// `ini_parser/tests.rs`, a directory name this file itself occupies.
#[path = "ini_parser_tests.rs"]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModListData {
    pub active_mods: Vec<String>,
    pub raw_ini_content: String,
}

/// Helper function to reliably resolve Zomboid/mods/default.txt path regardless of ini_path state
fn resolve_default_txt_path(ini_path: &str) -> PathBuf {
    if !ini_path.is_empty() {
        let p = Path::new(ini_path);
        if let Some(parent) = p.parent() {
            if parent.file_name().map_or(false, |n| n == "mods") {
                return parent.join("default.txt");
            } else if parent.file_name().map_or(false, |n| n == "Lua") {
                if let Some(pz_dir) = parent.parent() {
                    return pz_dir.join("mods").join("default.txt");
                }
            }
        }
    }
    // Fallback to standard user home Zomboid/mods/default.txt
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("C:\\"));
    home.join("Zomboid").join("mods").join("default.txt")
}

/// Which load-order file format a given file uses. Shared with the fix engine
/// so a plan preview and the actual writer can never disagree about syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOrderFormat {
    /// `mods.txt` — one mod id per line.
    PlainList,
    /// `mods/default.txt` — the native Lua table.
    DefaultTxt,
    /// `ModListData.ini` / `loadorder.ini` — semicolon-separated key.
    Ini,
}

impl LoadOrderFormat {
    /// Guess the format from a file name. `None` when we do not recognise it.
    pub fn from_file_name(name: &str) -> Option<LoadOrderFormat> {
        match name {
            "mods.txt" | "mod_order.txt" | "mod_load_order.txt"
            | "ModLoadOrderSorter.txt" | "ModLoadOrderExporter.txt" => Some(LoadOrderFormat::PlainList),
            "default.txt" | "reset-mods-42_00.txt" => Some(LoadOrderFormat::DefaultTxt),
            "ModListData.ini" | "loadorder.ini" | "modgroups.ini" => Some(LoadOrderFormat::Ini),
            _ => None,
        }
    }
}

pub(crate) fn parse_ini_text(content: &str) -> Vec<String> {
    let mut active = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("activeMods=") || trimmed.starts_with("Mods=") || trimmed.starts_with("mods=") {
            let parts: Vec<&str> = trimmed.split('=').collect();
            if parts.len() == 2 {
                active = parts[1]
                    .split(';')
                    .map(|s| sanitize_mod_id(s.trim()))
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    active
}

pub(crate) fn parse_default_txt_text(content: &str) -> Vec<String> {
    let mut active = Vec::new();
    let mut in_mods_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "mods" || trimmed.starts_with("mods") {
            in_mods_block = true;
            continue;
        }
        if in_mods_block && (trimmed == "maps" || trimmed.starts_with("maps")) {
            break;
        }

        if in_mods_block && trimmed.starts_with("mod") {
            let raw_id = trimmed[3..]
                .trim()
                .trim_start_matches('=')
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches(',')
                .trim();

            let clean_id = sanitize_mod_id(raw_id);
            if !clean_id.is_empty() && clean_id != "{" && clean_id != "}" {
                active.push(clean_id);
            }
        }
    }
    active
}

pub(crate) fn parse_plain_list_text(content: &str) -> Vec<String> {
    let mut active = Vec::new();
    for line in content.lines() {
        let clean = sanitize_mod_id(line);
        if !clean.is_empty() {
            active.push(clean);
        }
    }
    active
}

/// Render a mod list in Project Zomboid's native Lua table form.
pub(crate) fn render_default_txt(active_mods: &[String]) -> String {
    let mut out = String::from("VERSION = 1,\n\nmods\n{\n");
    for mod_id in active_mods {
        let clean_id = sanitize_mod_id(mod_id);
        if !clean_id.is_empty() {
            out.push_str(&format!("    mod = {},\n", clean_id));
        }
    }
    out.push_str("}\n\nmaps\n{\n}\n");
    out
}

/// Render a mod list as newline-separated ids.
pub(crate) fn render_plain_list(active_mods: &[String]) -> String {
    active_mods
        .iter()
        .map(|id| sanitize_mod_id(id))
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>()
        .join("\n")
}

/// Render a mod list as a single `key=a;b;c` ini line.
pub(crate) fn render_semicolon_list(active_mods: &[String]) -> String {
    active_mods
        .iter()
        .map(|id| sanitize_mod_id(id))
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>()
        .join(";")
}

/// Build the full text of an ini-format load-order file. `section` and `key`
/// must already be trusted literals, not user input.
pub(crate) fn render_ini(section: &str, key: &str, active_mods: &[String]) -> String {
    format!(
        "[{}]\n{}={}\n",
        section,
        key,
        render_semicolon_list(active_mods)
    )
}

fn parse_ini_file(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_ini_text(&content),
        Err(_) => Vec::new(),
    }
}

fn parse_default_txt(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_default_txt_text(&content),
        Err(_) => Vec::new(),
    }
}

fn parse_plain_list_file(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_plain_list_text(&content),
        Err(_) => Vec::new(),
    }
}

/// Reads active mods from Project Zomboid's most recently modified load order file on disk
/// (checking ModListData.ini, mods/default.txt, mods.txt, and loadorder.ini)
pub fn read_mod_list_ini(ini_path: &str) -> Result<ModListData, String> {
    let default_txt_path = resolve_default_txt_path(ini_path);
    let ini_p = Path::new(ini_path);

    let mut candidate_paths: Vec<(PathBuf, std::time::SystemTime, &str)> = Vec::new();

    // 1. ModListData.ini
    if !ini_path.is_empty() && ini_p.exists() {
        if let Ok(meta) = fs::metadata(ini_p) {
            if let Ok(mtime) = meta.modified() {
                candidate_paths.push((ini_p.to_path_buf(), mtime, "ini"));
            }
        }
    }

    // 2. mods/default.txt
    if default_txt_path.exists() {
        if let Ok(meta) = fs::metadata(&default_txt_path) {
            if let Ok(mtime) = meta.modified() {
                candidate_paths.push((default_txt_path.clone(), mtime, "default_txt"));
            }
        }
    }

    // 3. mods.txt
    if let Some(mods_dir) = default_txt_path.parent() {
        if let Some(z_dir) = mods_dir.parent() {
            let mods_txt_path = z_dir.join("mods.txt");
            if mods_txt_path.exists() {
                if let Ok(meta) = fs::metadata(&mods_txt_path) {
                    if let Ok(mtime) = meta.modified() {
                        candidate_paths.push((mods_txt_path, mtime, "plain"));
                    }
                }
            }

            // 4. loadorder.ini
            let loadorder_ini = z_dir.join("Lua").join("loadorder.ini");
            if loadorder_ini.exists() {
                if let Ok(meta) = fs::metadata(&loadorder_ini) {
                    if let Ok(mtime) = meta.modified() {
                        candidate_paths.push((loadorder_ini, mtime, "ini"));
                    }
                }
            }
        }
    }

    // Sort candidates by modification time DESCENDING (newest file modified on disk wins!)
    candidate_paths.sort_by(|a, b| b.1.cmp(&a.1));

    let mut active_mods = Vec::new();

    for (path, _mtime, file_type) in candidate_paths {
        let parsed = match file_type {
            "ini" => parse_ini_file(&path),
            "default_txt" => parse_default_txt(&path),
            "plain" => parse_plain_list_file(&path),
            _ => Vec::new(),
        };

        if !parsed.is_empty() {
            active_mods = parsed;
            break;
        }
    }

    Ok(ModListData {
        active_mods,
        raw_ini_content: String::new(),
    })
}

/// Writes active mod load order list back to Zomboid/mods/default.txt (Project Zomboid's actual primary active mods file)
/// in exact Project Zomboid Lua table format (safely quoting IDs with spaces or brackets), as well as ModListData.ini, loadorder.ini, ModManager, and modgroups.ini.
///
/// # Error handling (changed)
/// Previously every filesystem call was `let _ =`-swallowed and the function
/// returned `Ok(())` unconditionally, so a fully-failed save looked like a
/// successful one. It now **attempts every write** (one bad target must not
/// block the others) and then reports *every* failure, aggregated:
///
/// - `Ok(())` — every write succeeded.
/// - `Err(msg)` — at least one write failed. `msg` names each failing path and
///   the OS error, so the caller can tell the user which files are now stale.
///
/// Callers that genuinely treat a save as best-effort (`instance_manager`)
/// keep ignoring the `Result`; the fix engine checks it.
pub fn write_mod_list_ini(ini_path: &str, active_mods: &[String]) -> Result<(), String> {
    let default_txt_path = resolve_default_txt_path(ini_path);
    let mut target_zomboid_dirs = Vec::new();

    if let Some(mods_dir) = default_txt_path.parent() {
        if let Some(z_dir) = mods_dir.parent() {
            target_zomboid_dirs.push(z_dir.to_path_buf());
        }
    }

    let user_zomboid_str = if !ini_path.is_empty() {
        if let Some(parent) = Path::new(ini_path).parent() {
            if parent.file_name().map_or(false, |n| n == "mods") {
                parent.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
            } else if parent.file_name().map_or(false, |n| n == "Lua") {
                parent.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
            } else {
                parent.to_string_lossy().to_string()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    for u_dir in crate::load_order::mod_info::get_all_user_zomboid_dirs(&user_zomboid_str) {
        if !target_zomboid_dirs.contains(&u_dir) {
            target_zomboid_dirs.push(u_dir);
        }
    }

    // Everything below uses these renderers; the fix engine uses the same ones,
    // so a previewed write and the real save can never disagree about syntax.
    let default_txt_content = render_default_txt(active_mods);
    let active_lines = render_plain_list(active_mods);

    let ini_content = render_ini("ModList", "activeMods", active_mods);
    let loadorder_content = render_ini("LoadOrder", "mods", active_mods);
    let modgroups_content = render_ini("ModGroups", "active", active_mods);

    // Collected, not short-circuited: a partial save must still finish the rest.
    let mut errors: Vec<String> = Vec::new();
    let note_error = |errors: &mut Vec<String>, path: &Path, action: &str, e: std::io::Error| {
        errors.push(format!("{} {}: {}", action, path.to_string_lossy(), e));
    };

    for zomboid_dir in target_zomboid_dirs {
        // A. Primary Zomboid/mods/default.txt & lock file
        let mods_dir = zomboid_dir.join("mods");
        if let Err(e) = fs::create_dir_all(&mods_dir) {
            note_error(&mut errors, &mods_dir, "could not create directory", e);
        }
        let default_txt = mods_dir.join("default.txt");
        if let Err(e) = fs::write(&default_txt, &default_txt_content) {
            note_error(&mut errors, &default_txt, "could not write", e);
        }
        let reset_txt = mods_dir.join("reset-mods-42_00.txt");
        if let Err(e) = fs::write(&reset_txt, "If this file does not exist, default.txt will be reset to empty (no mods active).") {
            note_error(&mut errors, &reset_txt, "could not write", e);
        }

        // B. Root Zomboid/mods.txt
        let mods_txt = zomboid_dir.join("mods.txt");
        if let Err(e) = fs::write(&mods_txt, &active_lines) {
            note_error(&mut errors, &mods_txt, "could not write", e);
        }

        // C. Zomboid/Lua/mods/default.txt
        let lua_mods_dir = zomboid_dir.join("Lua").join("mods");
        if let Err(e) = fs::create_dir_all(&lua_mods_dir) {
            note_error(&mut errors, &lua_mods_dir, "could not create directory", e);
        }
        let lua_default_txt = lua_mods_dir.join("default.txt");
        if let Err(e) = fs::write(&lua_default_txt, &default_txt_content) {
            note_error(&mut errors, &lua_default_txt, "could not write", e);
        }
        let lua_reset_txt = lua_mods_dir.join("reset-mods-42_00.txt");
        if let Err(e) = fs::write(&lua_reset_txt, "If this file does not exist, default.txt will be reset to empty.") {
            note_error(&mut errors, &lua_reset_txt, "could not write", e);
        }

        // D. Zomboid/saved_modlists/ and Zomboid/Lua/saved_modlists/
        let saved_modlists_dir = zomboid_dir.join("saved_modlists");
        if let Err(e) = fs::create_dir_all(&saved_modlists_dir) {
            note_error(&mut errors, &saved_modlists_dir, "could not create directory", e);
        }
        for name in ["default.txt", "PZModStudio.txt", "PZ Mod Studio.txt"] {
            let p = saved_modlists_dir.join(name);
            if let Err(e) = fs::write(&p, &default_txt_content) {
                note_error(&mut errors, &p, "could not write", e);
            }
        }

        let lua_saved_dir = zomboid_dir.join("Lua").join("saved_modlists");
        if let Err(e) = fs::create_dir_all(&lua_saved_dir) {
            note_error(&mut errors, &lua_saved_dir, "could not create directory", e);
        }
        for name in ["default.txt", "PZModStudio.txt", "PZ Mod Studio.txt"] {
            let p = lua_saved_dir.join(name);
            if let Err(e) = fs::write(&p, &default_txt_content) {
                note_error(&mut errors, &p, "could not write", e);
            }
        }

        // E. Zomboid/Lua config files
        let lua_dir = zomboid_dir.join("Lua");
        if let Err(e) = fs::create_dir_all(&lua_dir) {
            note_error(&mut errors, &lua_dir, "could not create directory", e);
        }
        let e_files: [(&str, &String); 7] = [
            ("ModListData.ini", &ini_content),
            ("loadorder.ini", &loadorder_content),
            ("modgroups.ini", &modgroups_content),
            ("ModLoadOrderSorter.txt", &active_lines),
            ("mod_order.txt", &active_lines),
            ("mod_load_order.txt", &active_lines),
            ("ModLoadOrderExporter.txt", &active_lines),
        ];
        for (name, content) in e_files {
            let p = lua_dir.join(name);
            if let Err(e) = fs::write(&p, content) {
                note_error(&mut errors, &p, "could not write", e);
            }
        }

        // F. Zomboid/Lua/ModManager/ (In-game Mod Manager mod support)
        let mm_dir = lua_dir.join("ModManager");
        let mm_mods_dir = mm_dir.join("mods");
        if let Err(e) = fs::create_dir_all(&mm_mods_dir) {
            note_error(&mut errors, &mm_mods_dir, "could not create directory", e);
        }
        let mm_default_txt = mm_mods_dir.join("default.txt");
        if let Err(e) = fs::write(&mm_default_txt, &default_txt_content) {
            note_error(&mut errors, &mm_default_txt, "could not write", e);
        }
        let mm_reset_txt = mm_mods_dir.join("reset-mods-42_00.txt");
        if let Err(e) = fs::write(&mm_reset_txt, "If this file does not exist, default.txt will be reset to empty.") {
            note_error(&mut errors, &mm_reset_txt, "could not write", e);
        }

        let mm_saved_dir = mm_dir.join("saved_modlists");
        if let Err(e) = fs::create_dir_all(&mm_saved_dir) {
            note_error(&mut errors, &mm_saved_dir, "could not create directory", e);
        }
        for name in ["default.txt", "PZModStudio.txt", "PZ Mod Studio.txt"] {
            let p = mm_saved_dir.join(name);
            if let Err(e) = fs::write(&p, &default_txt_content) {
                note_error(&mut errors, &p, "could not write", e);
            }
        }

        let f_files: [(&str, &String); 6] = [
            ("loadorder.ini", &loadorder_content),
            ("modgroups.ini", &modgroups_content),
            ("ModLoadOrderSorter.txt", &active_lines),
            ("mod_order.txt", &active_lines),
            ("mod_load_order.txt", &active_lines),
            ("ModLoadOrderExporter.txt", &active_lines),
        ];
        for (name, content) in f_files {
            let p = mm_dir.join(name);
            if let Err(e) = fs::write(&p, content) {
                note_error(&mut errors, &p, "could not write", e);
            }
        }

        // G. Sync active mods to existing save game folders (Zomboid/Saves/*/*/mods.txt)
        let saves_dir = zomboid_dir.join("Saves");
        if saves_dir.exists() {
            for entry in walkdir::WalkDir::new(&saves_dir).max_depth(4).into_iter().filter_map(|e| e.ok()) {
                if entry.file_name() == "mods.txt" {
                    let p = entry.path().to_path_buf();
                    if let Err(e) = fs::write(&p, &default_txt_content) {
                        note_error(&mut errors, &p, "could not write", e);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of the load-order files could not be written: {}",
            errors.len(),
            errors.join("; ")
        ))
    }
}
