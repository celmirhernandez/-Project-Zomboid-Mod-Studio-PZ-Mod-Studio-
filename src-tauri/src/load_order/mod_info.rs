use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use crate::load_order::ini_parser::{read_mod_list_ini, write_mod_list_ini};
use crate::vfs::StudioPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub workshop_id: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub pzversion: Option<String>,
    pub url: Option<String>,
    pub require: Vec<String>,
    pub load_mod_after: Vec<String>,
    pub incompatible: Vec<String>,
    pub icon_path: Option<String>,
    pub poster_url: Option<String>,
    pub is_library: bool,
    pub is_map_mod: bool,
    pub enabled: bool,
    pub is_packaged: Option<bool>,
}

/// Helper function to sanitize and clean raw mod ID strings.
pub fn sanitize_mod_id(raw: &str) -> String {
    raw.trim()
        .trim_matches('\\')
        .trim_matches('/')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// Clean text encoding (fixes Windows-1252 / ANSI garbage characters like Ã±, Ã¡, etc.)
fn clean_text_encoding(text: &str) -> String {
    let mut cleaned = text.to_string();
    
    // Common UTF-8 misdecoded in Windows-1252 / Latin-1 replacements
    let replacements = [
        ("Ã¡", "á"), ("Ã©", "é"), ("Ã*", "í"), ("Ã³", "ó"), ("Ãº", "ú"),
        ("Ã±", "ñ"), ("Ã‘", "Ñ"), ("Ã?", "Á"), ("Ã‰", "É"), ("Ã?", "Í"),
        ("Ã“", "Ó"), ("Ãš", "Ú"), ("â€™", "'"), ("â€œ", "\""), ("â€", "\""),
        ("â€“", "-"), ("â€”", "—"), ("Â°", "°"), ("ï»¿", ""),
    ];

    for (from, to) in replacements {
        cleaned = cleaned.replace(from, to);
    }

    // Strip unprintable control characters
    cleaned.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect()
}

/// Helper to robustly read mod.info content handling UTF-8 with BOM, Latin-1, and Windows-1252
fn read_text_file_lossy(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    
    // Strip UTF-8 BOM if present
    let bytes_slice = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes[..]
    };

    let decoded = match String::from_utf8(bytes_slice.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            // Fallback to ISO-8859-1 / Windows-1252 lossy decoding
            bytes_slice.iter().map(|&b| b as char).collect()
        }
    };

    Some(clean_text_encoding(&decoded))
}

/// Parses a mod.info file into a structured ModManifest struct.
pub fn parse_mod_info(path: &Path) -> Option<ModManifest> {
    if !path.exists() {
        return None;
    }

    let content = read_text_file_lossy(path)?;
    let mut id = String::new();
    let mut name = String::new();
    let mut description = None;
    let mut author = None;
    let mut version = None;
    let mut pzversion = None;
    let mut url = None;
    let mut require = Vec::new();
    let mut load_mod_after = Vec::new();
    let mut incompatible = Vec::new();
    let mut poster_file = String::new();
    let mut icon_file = String::new();
    let mut pack = false;
    let mut tiledef = false;

    let parent_dir = path.parent();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("id=") {
            id = sanitize_mod_id(&trimmed[3..]);
        } else if trimmed.starts_with("name=") {
            name = clean_text_encoding(trimmed[5..].trim());
        } else if trimmed.starts_with("description=") {
            let desc_val = clean_text_encoding(trimmed[12..].trim());
            if !desc_val.is_empty() {
                description = Some(desc_val);
            }
        } else if trimmed.starts_with("author=") {
            let auth = clean_text_encoding(trimmed[7..].trim());
            if !auth.is_empty() {
                author = Some(auth);
            }
        } else if trimmed.starts_with("modversion=") {
            let v = trimmed[11..].trim().to_string();
            if !v.is_empty() {
                version = Some(v);
            }
        } else if trimmed.starts_with("version=") {
            let v = trimmed[8..].trim().to_string();
            if !v.is_empty() && version.is_none() {
                version = Some(v);
            }
        } else if trimmed.starts_with("pzversion=") {
            let v = trimmed[10..].trim().to_string();
            if !v.is_empty() {
                pzversion = Some(v);
            }
        } else if trimmed.starts_with("versionMin=") {
            let v = trimmed[11..].trim().to_string();
            if !v.is_empty() && pzversion.is_none() {
                pzversion = Some(v);
            }
        } else if trimmed.starts_with("url=") {
            let u = trimmed[4..].trim().to_string();
            if !u.is_empty() {
                url = Some(u);
            }
        } else if trimmed.starts_with("require=") {
            let req_str = trimmed[8..].trim();
            for s in req_str.split(',') {
                let clean = sanitize_mod_id(s);
                let lower = clean.to_lowercase();
                let is_junk = matches!(
                    lower.as_str(),
                    "please" | "update" | "to" | "b42" | "version" | "of" | "the" | "game" | "read" | "mod" | "page" | "for" | "help" | "visit" | "steam" | "link" | "download" | "notice" | "warning"
                );
                if !clean.is_empty() && !is_junk && !require.contains(&clean) {
                    require.push(clean);
                }
            }
        } else if let Some(req_str) = trimmed.strip_prefix("loadModAfter=")
            .or_else(|| trimmed.strip_prefix("load_mod_after="))
            .or_else(|| trimmed.strip_prefix("loadAfter="))
            .or_else(|| trimmed.strip_prefix("load_after="))
            .or_else(|| trimmed.strip_prefix("loadafter="))
            .or_else(|| trimmed.strip_prefix("loadmodafter="))
            .or_else(|| trimmed.strip_prefix("LoadAfter="))
            .or_else(|| trimmed.strip_prefix("LoadModAfter="))
        {
            for s in req_str.trim().split(',') {
                let clean = sanitize_mod_id(s);
                if !clean.is_empty() && !load_mod_after.contains(&clean) {
                    load_mod_after.push(clean);
                }
            }
        } else if trimmed.starts_with("incompatible=") {
            let inc_str = trimmed[13..].trim();
            incompatible = inc_str
                .split(',')
                .map(|s| sanitize_mod_id(s))
                .filter(|s| !s.is_empty())
                .collect();
        } else if trimmed.starts_with("incompatibleWith=") {
            let inc_str = trimmed[17..].trim();
            incompatible = inc_str
                .split(',')
                .map(|s| sanitize_mod_id(s))
                .filter(|s| !s.is_empty())
                .collect();
        } else if trimmed.starts_with("poster=") {
            poster_file = trimmed[7..].trim().to_string();
        } else if trimmed.starts_with("icon=") {
            icon_file = trimmed[5..].trim().to_string();
        } else if trimmed.starts_with("pack=") {
            pack = true;
        } else if trimmed.starts_with("tiledef=") {
            tiledef = true;
        }
    }

    if id.is_empty() {
        return None;
    }

    // Format synthetic package display name uniformly as "PZ Mod Studio Patch: <name>" across Mod List, Mod Merger, and game
    if id.starts_with("Z_PZModStudio_") && !id.contains("Carrier") {
        let clean_sub = if id.starts_with("Z_PZModStudio_") {
            id["Z_PZModStudio_".len()..].to_string().replace('_', " ")
        } else {
            name.clone()
        };
        name = format!("PZ Mod Studio Patch: {}", clean_sub);
    }

    // Robust search for poster and icon images in mod directory
    let mut poster_url = None;
    let mut icon_path = None;

    if let Some(dir) = parent_dir {
        let mut dirs_to_check = vec![dir.to_path_buf()];
        if let Some(p) = dir.parent() {
            dirs_to_check.push(p.to_path_buf());
            if let Some(gp) = p.parent() {
                dirs_to_check.push(gp.to_path_buf());
            }
        }

        for search_dir in &dirs_to_check {
            let poster_candidates = vec![
                if !poster_file.is_empty() { Some(search_dir.join(&poster_file)) } else { None },
                Some(search_dir.join("poster.png")),
                Some(search_dir.join("Poster.png")),
                Some(search_dir.join("poster.jpg")),
                Some(search_dir.join("Poster.jpg")),
                Some(search_dir.join("preview.png")),
                Some(search_dir.join("preview.jpg")),
                Some(search_dir.join("icon.png")),
            ];

            for cand in poster_candidates.into_iter().flatten() {
                if cand.exists() {
                    poster_url = Some(cand.to_string_lossy().to_string());
                    break;
                }
            }
            if poster_url.is_some() {
                break;
            }
        }

        for search_dir in &dirs_to_check {
            let icon_candidates = vec![
                if !icon_file.is_empty() { Some(search_dir.join(&icon_file)) } else { None },
                Some(search_dir.join("icon.png")),
                Some(search_dir.join("Icon.png")),
            ];

            for cand in icon_candidates.into_iter().flatten() {
                if cand.exists() {
                    icon_path = Some(cand.to_string_lossy().to_string());
                    break;
                }
            }
            if icon_path.is_some() {
                break;
            }
        }
    }

    let lower_id = id.to_lowercase();
    let is_addon_or_patch = lower_id.contains("addon") || lower_id.contains("patch") || lower_id.contains("compat") || lower_id.contains("ext");
    let is_library = !is_addon_or_patch && (
        lower_id.contains("lib") ||
        lower_id.contains("manager") ||
        lower_id.contains("framework") ||
        lower_id.contains("api") ||
        lower_id.contains("core")
    );
    let is_map_mod = pack || tiledef || lower_id.contains("map");

    let mut workshop_id = extract_workshop_id_from_path(path);
    if workshop_id.is_none() {
        if let Some(ref u) = url {
            if let Some(pos) = u.find("id=") {
                let rest = &u[pos + 3..];
                let w_id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !w_id.is_empty() {
                    workshop_id = Some(w_id);
                }
            }
        }
    }

    let mut manifest = ModManifest {
        id,
        name: if name.is_empty() { "Unnamed Mod".to_string() } else { name },
        description,
        workshop_id,
        author,
        version,
        pzversion,
        url,
        require,
        load_mod_after,
        incompatible,
        icon_path,
        poster_url,
        is_library,
        is_map_mod,
        enabled: false,
        is_packaged: None,
    };

    apply_known_dependency_heuristics(&mut manifest);
    Some(manifest)
}


pub fn apply_known_dependency_heuristics(manifest: &mut ModManifest) {
    let lower_id = manifest.id.to_lowercase();
    let w_id = manifest.workshop_id.as_deref().unwrap_or("");

    // 1. Brita's Weapon Pack (id: Brita, workshop: 2200148440)
    if lower_id == "brita" || w_id == "2200148440" {
        if !manifest.require.iter().any(|r| r.to_lowercase().contains("gunfighter")) {
            manifest.require.push("Arsenal(26)GunFighter[MAIN MOD 2.0]".to_string());
        }
        if !manifest.require.iter().any(|r| r.to_lowercase().contains("modoptions")) {
            manifest.require.push("modoptions".to_string());
        }
    }

    // 2. Arsenal(26) GunFighter (id: Arsenal(26)GunFighter[MAIN MOD 2.0], workshop: 2297098490)
    if lower_id.contains("gunfighter") || w_id == "2297098490" {
        if !manifest.require.iter().any(|r| r.to_lowercase().contains("modoptions")) {
            manifest.require.push("modoptions".to_string());
        }
        if lower_id.contains("2.0") || lower_id.contains("main") {
            if !manifest.incompatible.iter().any(|i| i.to_lowercase() == "arsenal(26)gunfighter") {
                manifest.incompatible.push("Arsenal(26)GunFighter".to_string());
            }
        } else {
            if !manifest.incompatible.iter().any(|i| i.to_lowercase().contains("main mod 2.0")) {
                manifest.incompatible.push("Arsenal(26)GunFighter[MAIN MOD 2.0]".to_string());
            }
        }
    }

    // 3. Brita's Armor Pack (id: Brita_Armor, workshop: 2460154811)
    if lower_id == "brita_armor" || w_id == "2460154811" {
        if !manifest.require.iter().any(|r| r.to_lowercase().contains("modoptions")) {
            manifest.require.push("modoptions".to_string());
        }
    }

    // 4. Tsar / Autotsar vehicle & trailer mods
    if lower_id.contains("autotsar") || lower_id.contains("aquatsar") || lower_id.contains("tsar") {
        if lower_id != "tsarslib" && !manifest.require.iter().any(|r| r.to_lowercase() == "tsarslib") {
            manifest.require.push("Tsarslib".to_string());
        }
    }

    // 5. Vehicle Scene Customization
    if lower_id.contains("vehiclescenecustomization") {
        if !manifest.require.iter().any(|r| r.to_lowercase() == "tsarslib") {
            manifest.require.push("Tsarslib".to_string());
        }
    }

    // Recalculate is_library
    manifest.is_library = manifest.require.is_empty() || lower_id.contains("lib") || lower_id.contains("manager") || lower_id.contains("framework");
}

pub fn get_all_user_zomboid_dirs(user_zomboid_dir: &str) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    let clean = user_zomboid_dir.trim();
    if !clean.is_empty() {
        dirs.push(std::path::PathBuf::from(clean));
    }
    if let Some(home) = dirs_next::home_dir() {
        let p1 = home.join("Zomboid");
        if !dirs.contains(&p1) { dirs.push(p1); }
        let p2 = home.join("Documents").join("Zomboid");
        if !dirs.contains(&p2) { dirs.push(p2); }
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let p3 = std::path::PathBuf::from(&user_profile).join("Zomboid");
        if !dirs.contains(&p3) { dirs.push(p3); }
        let p4 = std::path::PathBuf::from(&user_profile).join("Documents").join("Zomboid");
        if !dirs.contains(&p4) { dirs.push(p4); }
    }
    if dirs.is_empty() {
        dirs.push(std::path::PathBuf::from("C:\\Zomboid"));
    }
    dirs
}

pub fn ensure_master_patch_exists_on_disk(paths: &StudioPaths) {
    let mut target_dirs = Vec::new();
    if let Some(ref carrier_id) = paths.carrier_workshop_id {
        let clean_carrier = carrier_id.trim();
        if !clean_carrier.is_empty() && !paths.workshop_dir.is_empty() {
            target_dirs.push(Path::new(&paths.workshop_dir).join(clean_carrier).join("mods").join("Z_PZModStudio_MergedPatch"));
        }
    }
    for z_dir in get_all_user_zomboid_dirs(&paths.user_zomboid_dir) {
        target_dirs.push(z_dir.join("mods").join("Z_PZModStudio_MergedPatch"));
        target_dirs.push(z_dir.join("Lua").join("mods").join("Z_PZModStudio_MergedPatch"));
    }
    if !paths.pz_install_dir.is_empty() {
        target_dirs.push(Path::new(&paths.pz_install_dir).join("mods").join("Z_PZModStudio_MergedPatch"));
    }

    let mod_info_content = format!(
        "name={}\r\nid={}\r\ndescription={}\r\nposter=poster.png\r\nicon=icon.png\r\nmodversion=1.0.0\r\npzversion=41,42\r\nversionMin=41.00\r\nurl=https://github.com/Pichu6/-Project-Zomboid-Mod-Studio-PZ-Mod-Studio-\r\nauthor=PZ Mod Studio\r\n",
        "Z_PZModStudio Master Patch",
        "Z_PZModStudio_MergedPatch",
        "Auto-generated 3-Way compatibility patch and polyfill layer generated by Project Zomboid Mod Studio."
    );

    let polyfill_lua_content = crate::patch_generator::generate_master_polyfill_lua();

    for patch_dir in target_dirs {
        if fs::create_dir_all(&patch_dir).is_err() {
            continue;
        }

        let mod_info_path = patch_dir.join("mod.info");
        let _ = fs::write(&mod_info_path, &mod_info_content);
        let png_256 = crate::patch_generator::get_preview_png_bytes();
        let _ = fs::write(patch_dir.join("poster.png"), &png_256);
        let _ = fs::write(patch_dir.join("icon.png"), &png_256);

        let media_dir = patch_dir.join("media");
        let _ = fs::create_dir_all(&media_dir);

        let polyfill_dir = media_dir.join("lua").join("shared");
        let _ = fs::create_dir_all(&polyfill_dir);
        let _ = fs::write(polyfill_dir.join("Z_PZModStudio_Polyfills.lua"), &polyfill_lua_content);

        // Also ensure 42/ subfolder for native B42 multi-directory routing
        let dir_42 = patch_dir.join("42");
        let _ = fs::create_dir_all(&dir_42);
        let _ = fs::write(dir_42.join("mod.info"), &mod_info_content);
        let _ = fs::write(dir_42.join("poster.png"), &png_256);
        let _ = fs::write(dir_42.join("icon.png"), &png_256);

        let media_42_dir = dir_42.join("media");
        let _ = fs::create_dir_all(&media_42_dir);
        let polyfill_42_dir = media_42_dir.join("lua").join("shared");
        let _ = fs::create_dir_all(&polyfill_42_dir);
        let _ = fs::write(polyfill_42_dir.join("Z_PZModStudio_Polyfills.lua"), &polyfill_lua_content);
    }
}

/// Reads Steam's appworkshop_108600.acf manifest to retrieve the authoritative list of subscribed Workshop IDs.
pub fn get_subscribed_workshop_ids(workshop_dir: &Path) -> Option<HashSet<String>> {
    let mut candidates = Vec::new();
    if let Some(parent) = workshop_dir.parent().and_then(|p| p.parent()) {
        candidates.push(parent.join("appworkshop_108600.acf"));
        candidates.push(parent.join("appworkshop_108600.vdf"));
    }

    for candidate in candidates {
        if candidate.exists() {
            if let Ok(content) = fs::read_to_string(&candidate) {
                let mut subscribed_ids = HashSet::new();

                // 1. Try parsing WorkshopItemDetails first (most authoritative for active subscriptions)
                if content.contains("\"WorkshopItemDetails\"") {
                    let mut in_details = false;
                    let mut current_id = String::new();
                    let mut has_subscriber = false;
                    let mut brace_depth = 0;

                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.contains("\"WorkshopItemDetails\"") {
                            in_details = true;
                            continue;
                        }
                        if in_details {
                            if trimmed.starts_with('{') {
                                brace_depth += 1;
                            } else if trimmed.starts_with('}') {
                                if brace_depth == 2 {
                                    if !current_id.is_empty() && has_subscriber {
                                        subscribed_ids.insert(current_id.clone());
                                    }
                                    current_id.clear();
                                    has_subscriber = false;
                                }
                                brace_depth -= 1;
                                if brace_depth <= 0 {
                                    break;
                                }
                            } else if brace_depth == 1 {
                                let clean = trimmed.trim_matches('"').trim();
                                if clean.chars().all(|c| c.is_numeric()) && !clean.is_empty() {
                                    current_id = clean.to_string();
                                    has_subscriber = false;
                                }
                            } else if brace_depth == 2 {
                                if trimmed.contains("\"subscribedby\"") {
                                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                                    if let Some(val) = parts.last() {
                                        let sub_id = val.trim_matches('"').trim();
                                        if sub_id != "0" && !sub_id.is_empty() {
                                            has_subscriber = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if !subscribed_ids.is_empty() {
                    return Some(subscribed_ids);
                }

                // 2. Fallback to WorkshopItemsInstalled
                let mut installed_ids = HashSet::new();
                let mut in_installed = false;
                let mut brace_depth = 0;

                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains("\"WorkshopItemsInstalled\"") {
                        in_installed = true;
                        continue;
                    }
                    if in_installed {
                        if trimmed.starts_with('{') {
                            brace_depth += 1;
                        } else if trimmed.starts_with('}') {
                            brace_depth -= 1;
                            if brace_depth <= 0 {
                                break;
                            }
                        } else if brace_depth == 1 {
                            let clean = trimmed.trim_matches('"').trim();
                            if clean.chars().all(|c| c.is_numeric()) && !clean.is_empty() {
                                installed_ids.insert(clean.to_string());
                            }
                        }
                    }
                }

                if !installed_ids.is_empty() {
                    return Some(installed_ids);
                }
            }
        }
    }
    None
}

/// Returns version priority score for a mod.info path (e.g. 42.20 > 42.18 > 42 > common > root).
pub fn get_mod_info_version_score(path: &Path) -> u32 {
    if let Some(parent) = path.parent() {
        let dir_name = parent.file_name().unwrap_or_default().to_string_lossy();
        if dir_name.starts_with("42.") {
            if let Ok(minor) = dir_name[3..].parse::<u32>() {
                return 4200 + minor;
            }
            return 4200;
        } else if dir_name == "42" {
            return 4200;
        } else if dir_name == "common" {
            return 4000;
        }
    }
    1000
}

/// Raw discovery result: both the legacy (id-collapsed) view and the full
/// un-collapsed view, so callers that need to see duplicate installs can opt in
/// without changing the behavior every existing call site depends on.
pub struct DiscoveredMods {
    /// Exactly what `scan_all_installed_mods` has always returned: one entry
    /// per mod id, in resolved load order, remaining mods sorted alphabetically.
    pub ordered: Vec<ModManifest>,
    /// Every install found on disk, duplicates included, in discovery order
    /// (Workshop first, then local mods folders). This is what the conflict
    /// detector needs: two copies of the same mod id must both be visible.
    pub all_installs: Vec<ModManifest>,
}

/// What a scan produced, plus everything that went wrong along the way.
///
/// **One broken mod must never hide the rest.** Every per-mod failure while
/// walking the Workshop and local `mods` folders is collected here instead of
/// aborting the scan, so the caller can report `diagnostics` *and* `errors`
/// side by side. An unreadable `mod.info` no longer costs you the whole scan;
/// the user sees a clean list plus a list of files that need attention.
#[derive(Debug, Clone, Default)]
pub struct ModScanReport {
    /// Every install found on disk, duplicates included, in discovery order.
    pub all_installs: Vec<ModManifest>,
    /// One human-readable line per thing that could not be read or written,
    /// sorted and deduplicated. Empty means the scan saw no problems.
    pub errors: Vec<String>,
}

/// Whether a scan is allowed to touch the disk.
///
/// The historical scan repairs mod folders in place: it creates the build-42
/// sub-folder, copies `mod.info` into it, mirrors `media/`, and auto-installs the
/// Live Bridge companion mod. That is useful for a normal "scan my mods" action
/// but **forbidden** for a dry run — the fix engine's preview must be a pure
/// read, otherwise two previews of an unchanged install produce different plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// Repair mod folders in place, as the app has always done.
    Repairing,
    /// Read only. Never creates, copies or installs anything.
    ReadOnly,
}

impl ScanMode {
    fn may_write(self) -> bool {
        self == ScanMode::Repairing
    }
}

/// Scans all subscribed Workshop & local mods recursively without depth limits.
/// Preserves exact ModListData.ini load order for active mods, and sorts remaining mods deterministically.
pub fn scan_all_installed_mods(paths: &StudioPaths) -> Vec<ModManifest> {
    let mut errors = Vec::new();
    discover_installed_mods_with(paths, &mut errors, ScanMode::Repairing).ordered
}

/// Same scan as [`scan_all_installed_mods`] but WITHOUT collapsing manifests by
/// id, so a mod installed twice (e.g. once from the Workshop and once in the
/// local `mods` folder) yields two entries. Deterministic: discovery order is
/// preserved, and `enabled` is mirrored from the collapsed view.
///
/// Used by the conflict engine (`conflicts::detect_duplicate_mods`), which is
/// permanently blind if the id-keyed `HashMap` collapse happens first.
pub fn scan_all_installed_mods_list(paths: &StudioPaths) -> Vec<ModManifest> {
    let mut errors = Vec::new();
    discover_installed_mods_with(paths, &mut errors, ScanMode::Repairing).all_installs
}

/// A **pure read** of every installed mod: no folder is created, no `mod.info`
/// copied, no companion mod installed.
///
/// Use this for anything that must not have a side effect — the fix engine's
/// preview in particular, where a write during "previewing" would make the plan
/// un-hashable and break the staleness guarantee.
pub fn scan_all_installed_mods_read_only(paths: &StudioPaths) -> Vec<ModManifest> {
    let mut errors = Vec::new();
    discover_installed_mods_with(paths, &mut errors, ScanMode::ReadOnly).all_installs
}

/// The un-collapsed scan **plus** every per-mod error collected on the way.
///
/// This is the graceful path: a mod whose `mod.info` is unreadable, truncated,
/// or permission-denied is reported in `errors` while every other mod is still
/// returned in `all_installs`.
pub fn scan_all_installed_mods_with_errors(paths: &StudioPaths) -> ModScanReport {
    scan_all_installed_mods_in_mode(paths, ScanMode::Repairing)
}

/// [`scan_all_installed_mods_with_errors`] in a chosen [`ScanMode`]. Use
/// [`ScanMode::ReadOnly`] where a write would be wrong (see
/// [`scan_all_installed_mods_read_only`]).
pub fn scan_all_installed_mods_in_mode(paths: &StudioPaths, mode: ScanMode) -> ModScanReport {
    let mut errors: Vec<String> = Vec::new();
    let discovered = discover_installed_mods_with(paths, &mut errors, mode);
    // A deterministic, deduplicated error list: two directories with the same
    // broken file must not show up twice, and readdir order must not change
    // what the user sees.
    errors.sort();
    errors.dedup();
    ModScanReport {
        all_installs: discovered.all_installs,
        errors,
    }
}

fn note_scan_error(errors: &mut Vec<String>, path: &Path, what: &str) {
    if errors.len() > 500 {
        // Pathological trees must not be able to exhaust memory here.
        return;
    }
    errors.push(format!("{}: {}", path.to_string_lossy(), what));
}

/// `walkdir::Error::path()` is `None` for a few I/O failures that have no usable
/// path. Fall back to the root we were walking so the message still names
/// something the user can act on.
fn scan_root_for<'a>(path: Option<&'a Path>, root: &'a Path) -> &'a Path {
    path.unwrap_or(root)
}

fn discover_installed_mods_with(
    paths: &StudioPaths,
    errors: &mut Vec<String>,
    mode: ScanMode,
) -> DiscoveredMods {
    let mut all_mods_map: std::collections::HashMap<String, ModManifest> = std::collections::HashMap::new();
    let mut mod_scores: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    // Highest-scoring version dir already recorded per mod id, so a single
    // install with both `mod.info` and `42/mod.info` is not mistaken for a
    // duplicate install. Kept separate per source so that the SAME mod found
    // in both Workshop and local mods IS reported twice.
    let mut workshop_list: Vec<ModManifest> = Vec::new();
    let mut local_best_score: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut local_list: Vec<ModManifest> = Vec::new();

    // 1. Scan Steam Workshop mods (content/108600/) with depth 8
    let workshop_path = Path::new(&paths.workshop_dir);
    let subscribed_ids = get_subscribed_workshop_ids(workshop_path);

    if workshop_path.exists() {
        for entry in WalkDir::new(workshop_path).max_depth(8).into_iter() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    note_scan_error(errors, scan_root_for(e.path(), workshop_path), &format!("folder could not be read: {}", e));
                    continue;
                }
            };
            if entry.file_name() == "mod.info" {
                let mod_file_path = entry.path().to_path_buf();
                let workshop_id = extract_workshop_id_from_path(&mod_file_path);

                // If appworkshop_108600.acf is present, filter out unsubscribed orphaned folders!
                if let Some(ref subs) = subscribed_ids {
                    if let Some(ref w_id) = workshop_id {
                        if !subs.contains(w_id) {
                            continue; // Skip unsubscribed orphaned workshop mod
                        }
                    }
                }

                let is_already_version_dir = mod_file_path.iter().any(|comp| {
                    let s = comp.to_string_lossy();
                    s == "42" || s.starts_with("42.") || s == "common"
                });

                if !is_already_version_dir {
                    if let Some(parent) = mod_file_path.parent() {
                        let has_any_version_subdir = fs::read_dir(parent)
                            .map(|entries| {
                                entries.flatten().any(|e| {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    name == "42" || name.starts_with("42.") || name == "common"
                                })
                            })
                            .unwrap_or(false);

                        if mode.may_write() && !has_any_version_subdir {
                            let dir_42 = parent.join("42");
                            let info_42 = dir_42.join("mod.info");
                            if !info_42.exists() && &mod_file_path != &info_42 {
                                if let Err(e) = fs::create_dir_all(&dir_42) {
                                    note_scan_error(errors, &dir_42, &format!("could not create build-42 folder: {}", e));
                                }
                                if let Err(e) = fs::copy(&mod_file_path, &info_42) {
                                    note_scan_error(errors, &info_42, &format!("could not copy mod.info into the build-42 folder: {}", e));
                                }
                                let top_media = parent.join("media");
                                let dir_42_media = dir_42.join("media");
                                if top_media.exists() && !dir_42_media.exists() {
                                    if let Err(e) = copy_dir_all(&top_media, &dir_42_media) {
                                        note_scan_error(errors, &dir_42_media, &format!("could not copy the media folder: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }

                // A mod.info we cannot read must not cost the user the other
                // mods: record it and keep scanning.
                let parsed = parse_mod_info(&mod_file_path);
                if parsed.is_none() {
                    note_scan_error(errors, &mod_file_path, "could not be read as a mod.info (missing or malformed `id=` line)");
                }
                if let Some(mut manifest) = parsed {
                    if manifest.id == "PZModStudioCarrier" {
                        continue;
                    }
                    manifest.workshop_id = workshop_id;
                    apply_known_dependency_heuristics(&mut manifest);

                    let new_score = get_mod_info_version_score(&mod_file_path);
                    if let Some(existing_score) = mod_scores.get(&manifest.id) {
                        if new_score <= *existing_score {
                            continue; // Keep higher version-specific manifest (e.g. 42.20 > 42 > root)
                        }
                    }
                    mod_scores.insert(manifest.id.clone(), new_score);
                    workshop_list.push(manifest.clone());
                    all_mods_map.insert(manifest.id.clone(), manifest);
                }
            }
        }
    }

    // 2. Auto-install Live Bridge mod if missing in user's Zomboid/mods directory
    let user_dirs = get_all_user_zomboid_dirs(&paths.user_zomboid_dir);
    for user_dir in &user_dirs {
        if !mode.may_write() {
            break;
        }
        let bridge_dir = user_dir.join("mods").join("Z_PZModStudio_Bridge");
        if !bridge_dir.exists() {
            if let Err(e) = crate::sandbox::install_bridge_companion_mod(&user_dir.to_string_lossy()) {
                note_scan_error(
                    errors,
                    &bridge_dir,
                    &format!("Live Bridge companion mod could not be installed: {}", e),
                );
            }
        }
    }

    let active_from_disk = crate::patch_generator::get_active_mod_ids_from_disk(&paths.user_zomboid_dir, &paths.mod_list_ini_path);

    // 3. Scan User Zomboid mods folder across all candidate directories
    for user_dir in &user_dirs {
        let user_mods_path = user_dir.join("mods");
        if user_mods_path.exists() {
            for entry in WalkDir::new(&user_mods_path).max_depth(8).into_iter() {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        note_scan_error(errors, scan_root_for(e.path(), &user_mods_path), &format!("folder could not be read: {}", e));
                        continue;
                    }
                };
                if entry.file_name() == "mod.info" {
                    let parsed = parse_mod_info(entry.path());
                    if parsed.is_none() {
                        note_scan_error(
                            errors,
                            entry.path(),
                            "could not be read as a mod.info (missing or malformed `id=` line)",
                        );
                    }
                    if let Some(mut manifest) = parsed {
                        if manifest.id == "PZModStudioCarrier" {
                            continue;
                        }

                        // Check visibility for synthetic fusion packages and Live Bridge
                        if manifest.id.starts_with("Z_PZModStudio_") {
                            let mut mod_root = entry.path().parent().unwrap_or(entry.path());
                            while let Some(parent) = mod_root.parent() {
                                let folder_name = mod_root.file_name().unwrap_or_default().to_string_lossy();
                                if folder_name.starts_with("Z_PZModStudio_") {
                                    break;
                                }
                                if folder_name == "mods" || folder_name.is_empty() {
                                    break;
                                }
                                mod_root = parent;
                            }

                            let meta_path = mod_root.join("patch_metadata.json");
                            let is_active = active_from_disk.contains(&manifest.id);
                            let mut is_visible = is_active;
                            let mut is_pkg = false;

                            if meta_path.exists() {
                                if let Ok(meta_str) = fs::read_to_string(&meta_path) {
                                    if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                                        if meta_json["is_packaged"].as_bool() == Some(true) {
                                            is_visible = true;
                                            is_pkg = true;
                                        }
                                        if let Some(vis) = meta_json["is_visible_in_modlist"].as_bool() {
                                            is_visible = vis;
                                        }
                                    }
                                }
                            } else if manifest.id == "Z_PZModStudio_Bridge" {
                                is_visible = true;
                                is_pkg = true;
                            }

                            manifest.is_packaged = Some(is_pkg);

                            if !is_visible && !is_active {
                                continue; // Skip hidden package
                            }
                        }

                        // Preserve workshop_id, poster, and icon if previously scanned from Steam Workshop
                        if let Some(existing) = all_mods_map.get(&manifest.id) {
                            if manifest.workshop_id.is_none() {
                                manifest.workshop_id = existing.workshop_id.clone();
                            }
                            if manifest.poster_url.is_none() {
                                manifest.poster_url = existing.poster_url.clone();
                            }
                            if manifest.icon_path.is_none() {
                                manifest.icon_path = existing.icon_path.clone();
                            }
                        }

                        // Un-collapsed list: keep the highest-scoring version dir
                        // per id *within local folders only*, so a second install of
                        // the same mod (Workshop vs local) stays visible as a duplicate.
                        let local_score = get_mod_info_version_score(entry.path());
                        let record = match local_best_score.get(&manifest.id) {
                            Some(prev) => local_score > *prev,
                            None => true,
                        };
                        if record {
                            local_best_score.insert(manifest.id.clone(), local_score);
                            local_list.push(manifest.clone());
                        }

                        all_mods_map.insert(manifest.id.clone(), manifest);
                    }
                }
            }
        }
    }

    // 3. Check for Active Profile or PZModStudio_MasterLoadOrder.json to restore exact absolute load order
    let mut saved_load_order: Option<Vec<String>> = None;
    let mut saved_active_ids: Option<HashSet<String>> = None;

    let user_dirs = get_all_user_zomboid_dirs(&paths.user_zomboid_dir);
    for u_dir in &user_dirs {
        // A) Check PZModStudio_Instances/ for an active profile
        let inst_dir = u_dir.join("PZModStudio_Instances");
        if inst_dir.exists() {
            if let Ok(entries) = fs::read_dir(&inst_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Ok(c) = fs::read_to_string(&path) {
                            if let Err(e) = serde_json::from_str::<crate::instance_manager::AppInstance>(&c) {
                                // A corrupt profile must not hide every other profile.
                                note_scan_error(errors, &path, &format!("saved profile could not be read: {}", e));
                                continue;
                            }
                            if let Ok(inst) = serde_json::from_str::<crate::instance_manager::AppInstance>(&c) {
                                if inst.is_active && !inst.load_order.is_empty() {
                                    saved_load_order = Some(inst.load_order);
                                    saved_active_ids = Some(inst.active_mod_ids.into_iter().collect());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        if saved_load_order.is_some() {
            break;
        }

        // B) Check PZModStudio_MasterLoadOrder.json
        let master_path = u_dir.join("PZModStudio_MasterLoadOrder.json");
        if master_path.exists() {
            if let Ok(c) = fs::read_to_string(&master_path) {
                if serde_json::from_str::<serde_json::Value>(&c).is_err() {
                    note_scan_error(errors, &master_path, "saved master load order could not be read as JSON");
                }
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&c) {
                    if let Some(arr) = json_val["load_order"].as_array() {
                        let order: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        if !order.is_empty() {
                            saved_load_order = Some(order);
                            let active: HashSet<String> = json_val["active_mod_ids"]
                                .as_array()
                                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                .unwrap_or_default();
                            saved_active_ids = Some(active);
                            break;
                        }
                    }
                }
            }
        }
    }

    let mut result_mods = Vec::new();
    let mut processed_ids = HashSet::new();

    if let Some(order) = saved_load_order {
        let active_set = saved_active_ids.unwrap_or_default();
        for raw_id in order {
            let clean_id = sanitize_mod_id(&raw_id);
            let matched_key = all_mods_map
                .keys()
                .find(|k| {
                    let clean_k = k.trim().to_lowercase();
                    let clean_act = clean_id.trim().to_lowercase();
                    if clean_k == clean_act {
                        return true;
                    }
                    let alpha_k: String = clean_k.chars().filter(|c| c.is_alphanumeric()).collect();
                    let alpha_act: String = clean_act.chars().filter(|c| c.is_alphanumeric()).collect();
                    if !alpha_k.is_empty() && alpha_k == alpha_act {
                        return true;
                    }
                    false
                })
                .cloned();

            if let Some(key) = matched_key {
                if let Some(mut manifest) = all_mods_map.remove(&key) {
                    manifest.enabled = active_set.contains(&raw_id) || active_set.contains(&manifest.id) || active_set.contains(&key);
                    result_mods.push(manifest);
                    processed_ids.insert(key);
                }
            }
        }
    } else if let Ok(mut ini_data) = read_mod_list_ini(&paths.mod_list_ini_path) {
        let mut modified_ini = false;
        for raw_active_id in &ini_data.active_mods {
            let active_id = sanitize_mod_id(raw_active_id);

            // Case-insensitive & sanitized matching against scanned manifests
            let matched_key = all_mods_map
                .keys()
                .find(|k| {
                    let clean_k = k.trim().to_lowercase();
                    let clean_act = active_id.trim().to_lowercase();
                    if clean_k == clean_act {
                        return true;
                    }
                    let alpha_k: String = clean_k.chars().filter(|c| c.is_alphanumeric()).collect();
                    let alpha_act: String = clean_act.chars().filter(|c| c.is_alphanumeric()).collect();
                    if !alpha_k.is_empty() && alpha_k == alpha_act {
                        return true;
                    }
                    false
                })
                .cloned();

            if let Some(key) = matched_key {
                if let Some(mut manifest) = all_mods_map.remove(&key) {
                    // Open / Draft synthetic packages cannot be active in the game!
                    if manifest.id.starts_with("Z_PZModStudio_") && manifest.id != "Z_PZModStudio_Bridge" && manifest.is_packaged == Some(false) {
                        manifest.enabled = false;
                        all_mods_map.insert(key, manifest);
                        modified_ini = true;
                    } else {
                        manifest.enabled = true;
                        result_mods.push(manifest);
                        processed_ids.insert(key);
                    }
                }
            } else if active_id.starts_with("Z_PZModStudio_") {
                // This synthetic package is currently in DRAFT (is_packaged = false) or was deleted.
                // Do NOT include it in result_mods, and mark ini for pruning!
                modified_ini = true;
            }
            // If mod was unsubscribed and no longer exists on disk, skip it so it is cleaned up!
        }

        if modified_ini && !paths.mod_list_ini_path.is_empty() {
            ini_data.active_mods.retain(|id| !id.starts_with("Z_PZModStudio_") || processed_ids.contains(id));
            let _ = write_mod_list_ini(&paths.mod_list_ini_path, &ini_data.active_mods);
        }
    }

    // 4. Append remaining subscribed mods found on disk (disabled by default) in deterministic alphabetical order
    let mut remaining_mods: Vec<ModManifest> = all_mods_map
        .into_iter()
        .filter(|(id, _)| !processed_ids.contains(id))
        .map(|(_, mut manifest)| {
            manifest.enabled = false;
            manifest
        })
        .collect();

    // Sort remaining mods alphabetically by name and id so their position is 100% stable across refreshes!
    remaining_mods.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });

    result_mods.extend(remaining_mods);

    // Un-collapsed view: Workshop installs first, then local ones. `WalkDir`
    // gives no cross-platform order guarantee, so sort for determinism.
    let mut all_installs: Vec<ModManifest> = workshop_list;
    all_installs.extend(local_list);
    all_installs.sort_by(|a, b| {
        a.id.to_lowercase()
            .cmp(&b.id.to_lowercase())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    // `enabled` is resolved in the collapsed view only; mirror it back so the
    // diagnostics engine sees the same activation state the UI shows.
    for m in all_installs.iter_mut() {
        if let Some(canonical) = result_mods.iter().find(|c| c.id == m.id) {
            m.enabled = canonical.enabled;
        }
    }

    DiscoveredMods {
        ordered: result_mods,
        all_installs,
    }
}

pub fn extract_workshop_id_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    let normalized = path_str.replace('/', "\\");

    // 1. Check path containing 108600\
    if let Some(idx) = normalized.find("108600\\") {
        let rest = &normalized[idx + 7..];
        let w_id = rest.split('\\').next().unwrap_or("").trim();
        if !w_id.is_empty() && w_id.chars().all(|c| c.is_ascii_digit()) {
            return Some(w_id.to_string());
        }
    }

    // 2. Check path containing content\108600\
    if let Some(idx) = normalized.to_lowercase().find("content\\108600\\") {
        let rest = &normalized[idx + 15..];
        let w_id = rest.split('\\').next().unwrap_or("").trim();
        if !w_id.is_empty() && w_id.chars().all(|c| c.is_ascii_digit()) {
            return Some(w_id.to_string());
        }
    }

    None
}


fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().eq_ignore_ascii_case("AnimSets") {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(&name))?;
        } else {
            fs::copy(entry.path(), dst.join(&name))?;
        }
    }
    Ok(())
}

/// Converts B41-format `recipe { }` blocks to B42 `craftRecipe { }` format.
/// Scans all .txt files in src_scripts_dir, converts any recipe blocks found,
/// and writes converted files into dst_scripts_dir (under a `recipes/` subfolder).
#[allow(dead_code)]
fn convert_b41_recipes_to_b42(src_scripts_dir: &Path, dst_scripts_dir: &Path) -> std::io::Result<()> {
    for entry in WalkDir::new(src_scripts_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "txt" { continue; }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Only process files that contain B41 recipe blocks
        if !content.contains("\trecipe") && !content.contains(" recipe ") {
            continue;
        }

        // Skip files that already have craftRecipe (already B42)
        if content.contains("craftRecipe") {
            continue;
        }

        let converted = convert_script_recipes(&content);
        if converted.is_empty() { continue; }

        // Write to dst_scripts_dir/recipes/<filename>
        let recipes_dir = dst_scripts_dir.join("recipes");
        let _ = fs::create_dir_all(&recipes_dir);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("recipes");
        let out_path = recipes_dir.join(format!("{}_b42.txt", stem));

        // Don't overwrite if already converted
        if out_path.exists() { continue; }

        let _ = fs::write(&out_path, converted);
    }
    Ok(())
}

/// Parse a single B41 recipe block and convert to B42 craftRecipe format.
/// Returns the converted string, or empty string if conversion fails.
#[allow(dead_code)]
fn convert_script_recipes(content: &str) -> String {
    let mut output = String::new();
    let mut module_name = "Base".to_string();

    // Extract module name
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("module ") {
            module_name = trimmed
                .trim_start_matches("module ")
                .trim_end_matches('{')
                .trim_end_matches("	{")
                .trim().to_string();
            break;
        }
    }

    output.push_str(&format!("module {} {{\n", module_name));
    let mut recipes_written = 0;
    let mut seen_names: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    // Find all recipe blocks
    let mut pos = 0;
    while pos < content.len() {
        // Find next `recipe` keyword (not `craftRecipe`)
        let search = &content[pos..];
        // Look for `\trecipe ` or `\n recipe ` or `recipe \t`
        let recipe_marker = find_recipe_start(search);
        if recipe_marker.is_none() { break; }
        let offset = recipe_marker.unwrap();
        let abs_pos = pos + offset;

        // Extract recipe name (text between `recipe ` and `{`)
        let after_recipe = &content[abs_pos..];
        // Skip `recipe ` word
        let after_keyword = if let Some(kw_end) = after_recipe.find('{')
            .and_then(|_| {
                // find end of `recipe` keyword
                after_recipe.find(" ").or_else(|| after_recipe.find("\t"))
            }) {
            &after_recipe[kw_end..]
        } else {
            pos = abs_pos + 1;
            continue;
        };

        let recipe_name = match after_keyword.find('{') {
            Some(brace) => after_keyword[..brace].trim().to_string(),
            None => { pos = abs_pos + 1; continue; }
        };

        // Find the opening brace
        let block_start = match after_recipe.find('{') {
            Some(b) => abs_pos + b + 1,
            None => { pos = abs_pos + 1; continue; }
        };

        // Find the matching closing brace (counting nested braces)
        let mut depth = 1usize;
        let mut block_end = block_start;
        for (i, ch) in content[block_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        block_end = block_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 { pos = abs_pos + 1; continue; }

        let block_content = &content[block_start..block_end];
        pos = block_end + 1;

        // Skip if recipe_name is empty or looks like junk
        let raw_clean = recipe_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect::<String>();
        let clean_base = raw_clean.trim_matches('_').to_string();
        if clean_base.is_empty() { continue; }

        // Deduplicate recipe names
        let clean_name = {
            let count = seen_names.entry(clean_base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                clean_base
            } else {
                format!("{}_{}" , clean_base, count)
            }
        };

        // Parse block fields
        let mut time_val = "150".to_string();
        let mut category_val = "Weaponry".to_string();
        let mut on_create: Option<String> = None;
        let mut need_to_learn = "false".to_string();
        // inputs: (count, item_str, is_keep)
        let mut inputs: Vec<(u32, String, bool)> = Vec::new();
        // outputs: (count, item_str)  — NO brackets in B42 outputs
        let mut outputs: Vec<(u32, String)> = Vec::new();

        let skip_keys = [
            "result", "time", "category", "sound", "ontest", "oncreate",
            "nearitem", "skillrequired", "xpaward", "needtobelearn",
            "canbedonefloor", "canbedonefromfloor", "islearnable",
            "autolearn", "autolearnall", "workstation",
        ];

        for line in block_content.lines() {
            let trimmed = line.trim().trim_end_matches(',');
            if trimmed.is_empty() { continue; }
            if trimmed.starts_with("/*") || trimmed.starts_with("//") || trimmed.starts_with('*') { continue; }

            let colon_pos = trimmed.find(':');
            let equals_pos = trimmed.find('=');

            // Key: Value style (colon before any equals, or only colon)
            if let Some(cp) = colon_pos {
                if equals_pos.map_or(true, |ep| cp < ep) {
                    let key = trimmed[..cp].trim().to_lowercase();
                    let val = trimmed[cp+1..].trim();
                    match key.as_str() {
                        "time" => {
                            if let Ok(f) = val.trim_end_matches(".0").parse::<f64>() {
                                time_val = if f == f.trunc() { format!("{}", f as i64) } else { val.to_string() };
                            } else {
                                time_val = val.trim_end_matches(".0").to_string();
                            }
                        }
                        "category" => {
                            category_val = match val.to_lowercase().as_str() {
                                "firearm" | "weaponry" => "Weaponry",
                                "carpentry" => "Carpentry",
                                "cooking" => "Cooking",
                                "farming" => "Farming",
                                "firstaid" => "FirstAid",
                                "mechanics" => "Mechanics",
                                "metalwork" => "Metalwork",
                                "tailoring" => "Tailoring",
                                "electrical" => "Electrical",
                                _ => "Weaponry",
                            }.to_string();
                        }
                        "result" => {
                            let out_raw = val.trim();
                            if !out_raw.is_empty() {
                                // Handle Result:ItemName=N (with count) or Result:ItemName
                                let (item_name, count) = if let Some(eq_pos) = out_raw.find('=') {
                                    let name = out_raw[..eq_pos].trim();
                                    let cnt: u32 = out_raw[eq_pos+1..].trim().parse().unwrap_or(1);
                                    (name, cnt)
                                } else {
                                    (out_raw, 1)
                                };
                                let qualified = if item_name.contains('.') {
                                    item_name.to_string()
                                } else {
                                    format!("Base.{}", item_name)
                                };
                                outputs.push((count, qualified));
                            }
                        }
                        "oncreate" => on_create = Some(val.to_string()),
                        "needtobelearn" => {
                            need_to_learn = if val.to_lowercase() == "true" { "true" } else { "false" }.to_string();
                        }
                        _ => {}
                    }
                    continue;
                }
            }

            // Item = N  style (quantity input) or destroy Item = N
            if let Some(ep) = equals_pos {
                let lhs = trimmed[..ep].trim();
                let rhs = trimmed[ep+1..].trim();

                let is_keep = lhs.to_lowercase().starts_with("keep ");
                let lhs2 = if is_keep { lhs[5..].trim() } else { lhs };

                // Handle 'destroy ItemName = N' modifier
                let (item_part, mode_override) = if lhs2.to_lowercase().starts_with("destroy ") {
                    (lhs2[8..].trim(), Some("mode:destroy"))
                } else {
                    (lhs2, None)
                };

                // If key is a known property name, skip
                if skip_keys.contains(&item_part.to_lowercase().as_str()) { continue; }
                // Skip if contains space (property lines) but not OR groups
                if item_part.contains(':') { continue; }
                if item_part.contains(' ') && !item_part.contains('/') { continue; }

                let count: u32 = rhs.trim().parse().unwrap_or(1);
                let item_str = item_part.split('/')
                    .map(|p| {
                        let p = p.trim();
                        // Handle Recipe.GetItemTypes.X -> tags[base:x]
                        if p.starts_with("Recipe.GetItemTypes.") {
                            let type_name = &p[19..];
                            match type_name.to_lowercase().as_str() {
                                "petrol" => "tags[base:petrol]".to_string(),
                                "saw" => "tags[base:saw]".to_string(),
                                "screwdriver" => "tags[base:screwdriver]".to_string(),
                                "hammer" => "tags[base:hammer]".to_string(),
                                "pliers" => "tags[base:pliers]".to_string(),
                                "knife" => "tags[base:knife]".to_string(),
                                t => format!("tags[base:{}]", t.to_lowercase()),
                            }
                        } else if p.contains('.') {
                            p.to_string()
                        } else {
                            format!("Base.{}", p)
                        }
                    })
                    .collect::<Vec<_>>().join(";");
                if !item_str.is_empty() {
                    // If mode_override (destroy), encode as special keep flag
                    // In B42 mode:destroy means consumed
                    // We encode destroy as !is_keep so it appears without mode:keep
                    let _ = mode_override; // destroy mode handled by default (no mode:keep = consumed)
                    inputs.push((count, item_str, is_keep));
                }
                continue;
            }

            // Plain item: "ItemName" or "keep ItemName" or "keep A/B/C"
            let is_keep = trimmed.to_lowercase().starts_with("keep ");
            let trimmed2 = if is_keep { trimmed[5..].trim() } else { trimmed };

            // Handle 'destroy ItemName' modifier (no count)
            let item_part = if trimmed2.to_lowercase().starts_with("destroy ") {
                trimmed2[8..].trim()
            } else {
                trimmed2
            };

            if item_part.is_empty()
                || item_part.starts_with('/')
                || item_part.starts_with('*')
                || item_part.starts_with('#')
                || item_part.contains(' ')  // lines with spaces aren't item names
                || skip_keys.contains(&item_part.to_lowercase().as_str())
            { continue; }

            let item_str = item_part.split('/')
                .map(|p| {
                    let p = p.trim();
                    if p.contains('.') { p.to_string() } else { format!("Base.{}", p) }
                })
                .collect::<Vec<_>>().join(";");
            if !item_str.is_empty() {
                inputs.push((1, item_str, is_keep));
            }
        }

        // Build craftRecipe block
        let mut craft = String::new();
        craft.push_str(&format!("\n    craftRecipe {} {{\n", clean_name));
        craft.push_str(&format!("        time = {},\n", time_val));
        craft.push_str(&format!("        category = {},\n", category_val));
        craft.push_str("        Tags = AnySurfaceCraft,\n");
        craft.push_str(&format!("        NeedToBeLearn = {},\n", need_to_learn));
        if let Some(ref oc) = on_create {
            craft.push_str(&format!("        OnCreate = {},\n", oc));
        }

        // Inputs block — items WITH brackets []
        if !inputs.is_empty() {
            craft.push_str("        inputs\n        {\n");
            for (count, item_str, is_keep) in &inputs {
                let mode = if *is_keep { " mode:keep" } else { "" };
                craft.push_str(&format!("            item {} [{}]{},\n", count, item_str, mode));
            }
            craft.push_str("        }\n");
        }

        // Outputs block — items WITHOUT brackets
        if !outputs.is_empty() {
            craft.push_str("        outputs\n        {\n");
            for (count, item_str) in &outputs {
                craft.push_str(&format!("            item {} {},\n", count, item_str));
            }
            craft.push_str("        }\n");
        } else if on_create.is_some() {
            craft.push_str("        outputs\n        {\n        }\n");
        }

        craft.push_str("    }\n");
        output.push_str(&craft);
        recipes_written += 1;
    }

    output.push_str("}\n");

    if recipes_written == 0 { String::new() } else { output }
}

/// Find the start position of the next `recipe ` keyword in text,
/// skipping `craftRecipe` occurrences.
#[allow(dead_code)]
fn find_recipe_start(text: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < text.len() {
        // Look for `recipe` in text
        let found = text[search_from..].find("recipe ");
        if found.is_none() { return None; }
        let rel_pos = found.unwrap();
        let abs = search_from + rel_pos;

        // Make sure it's not `craftRecipe`
        if abs >= 5 && &text[abs-5..abs] == "craft" {
            search_from = abs + 7;
            continue;
        }
        // Make sure previous char is whitespace/newline/tab (word boundary)
        if abs > 0 {
            let prev = text.as_bytes()[abs - 1];
            if !prev.is_ascii_whitespace() {
                search_from = abs + 7;
                continue;
            }
        }
        return Some(abs);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribed_workshop_ids_filtering() {
        let workshop_dir = Path::new("G:/Juegos/steamapps/workshop/content/108600");
        if workshop_dir.exists() {
            let subs = get_subscribed_workshop_ids(workshop_dir);
            assert!(subs.is_some());
            let ids = subs.unwrap();
            assert!(!ids.is_empty(), "Must find active subscribed mods");
        }
    }

    #[test]
    fn test_bandits_week_one_version_resolution() {
        let root_info = Path::new("G:/Juegos/steamapps/workshop/content/108600/3403180543/mods/BanditsWeekOne/mod.info");
        let v42_20_info = Path::new("G:/Juegos/steamapps/workshop/content/108600/3403180543/mods/BanditsWeekOne/42.20/mod.info");

        if root_info.exists() && v42_20_info.exists() {
            let root_score = get_mod_info_version_score(root_info);
            let v42_20_score = get_mod_info_version_score(v42_20_info);
            assert!(v42_20_score > root_score);

            let manifest = parse_mod_info(v42_20_info).expect("Must parse 42.20 mod.info");
            assert_eq!(manifest.require, vec!["Bandits2".to_string()]);
        }
    }

    // -----------------------------------------------------------------------
    // Graceful scanning (P3): one bad mod must not block the others.
    // -----------------------------------------------------------------------

    struct ScanSandbox {
        root: std::path::PathBuf,
    }

    impl ScanSandbox {
        fn new(tag: &str) -> ScanSandbox {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!("pzms_scan_{}_{}", tag, nanos));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("mods")).expect("must create the sandbox");
            ScanSandbox { root }
        }

        fn path(&self) -> crate::vfs::StudioPaths {
            crate::vfs::StudioPaths {
                pz_install_dir: String::new(),
                workshop_dir: String::new(),
                user_zomboid_dir: self.root.to_string_lossy().to_string(),
                mod_list_ini_path: self
                    .root
                    .join("mods")
                    .join("ModListData.ini")
                    .to_string_lossy()
                    .to_string(),
                carrier_workshop_id: None,
                is_valid: true,
            }
        }

        fn mod_info(&self, folder: &str, body: &str) {
            let dir = self.root.join("mods").join(folder);
            fs::create_dir_all(&dir).expect("must create the mod folder");
            fs::write(dir.join("mod.info"), body).expect("must write the fixture");
        }
    }

    impl Drop for ScanSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_broken_mod_info_is_reported_without_hiding_the_good_mods() {
        let sb = ScanSandbox::new("broken_one");
        sb.mod_info("GoodA", "name=GoodA\nid=GoodA\n");
        // No `id=` line at all: unparseable.
        sb.mod_info("Broken", "name=Broken\nauthor=nobody\n");
        sb.mod_info("GoodB", "name=GoodB\nid=GoodB\n");

        let report = scan_all_installed_mods_with_errors(&sb.path());

        let ids: Vec<String> = report.all_installs.iter().map(|m| m.id.clone()).collect();
        assert!(
            ids.contains(&"GoodA".to_string()) && ids.contains(&"GoodB".to_string()),
            "one broken mod must not hide the rest, got {:?}",
            ids
        );
        assert!(
            report.errors.iter().any(|e| e.contains("Broken")),
            "the broken mod must be named in the errors, got {:?}",
            report.errors
        );
    }

    #[test]
    fn scan_errors_are_sorted_and_deduplicated() {
        let sb = ScanSandbox::new("sorted");
        sb.mod_info("Broken1", "no id here\n");
        sb.mod_info("Broken2", "also no id\n");

        let a = scan_all_installed_mods_with_errors(&sb.path()).errors;
        let mut expected = a.clone();
        expected.sort();
        expected.dedup();
        assert_eq!(a, expected, "scan errors must be sorted and deduplicated");
    }

    #[test]
    fn a_completely_empty_folder_scans_clean() {
        let sb = ScanSandbox::new("empty");
        // Read-only: the repairing scan deliberately auto-installs the Live
        // Bridge companion mod, so it would legitimately find one.
        //
        // The scan also walks every other candidate Zomboid folder on this
        // machine, so the result set cannot be asserted empty here — what this
        // test actually pins down is totality: an empty folder must scan without
        // panicking, and must not contribute a fabricated mod of its own.
        let report = scan_all_installed_mods_in_mode(&sb.path(), ScanMode::ReadOnly);
        assert!(
            report.all_installs.iter().all(|m| !m.id.is_empty()),
            "a scan must never invent a mod with an empty id"
        );
        // Nothing was created inside the sandbox.
        let installed: Vec<String> = fs::read_dir(sb.root.join("mods"))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            installed.is_empty(),
            "a read-only scan must not populate the folder, got {:?}",
            installed
        );
    }

    #[test]
    fn read_only_scan_creates_nothing() {
        let sb = ScanSandbox::new("readonly");
        sb.mod_info("Good", "name=Good\nid=Good\n");
        let before: Vec<String> = fs::read_dir(sb.root.join("mods"))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();

        let report = scan_all_installed_mods_in_mode(&sb.path(), ScanMode::ReadOnly);
        assert!(!report.all_installs.is_empty());

        let after: Vec<String> = fs::read_dir(sb.root.join("mods"))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        assert_eq!(
            before, after,
            "a read-only scan must not install or create anything"
        );
    }

    #[test]
    fn a_corrupt_saved_profile_does_not_hide_the_other_mods() {
        let sb = ScanSandbox::new("corrupt_profile");
        sb.mod_info("GoodA", "name=GoodA\nid=GoodA\n");
        sb.mod_info("GoodB", "name=GoodB\nid=GoodB\n");

        let inst_dir = sb.root.join("PZModStudio_Instances");
        fs::create_dir_all(&inst_dir).expect("must create the instances dir");
        fs::write(inst_dir.join("inst_broken.json"), "{ not json at all ")
            .expect("must write the corrupt profile");

        let report = scan_all_installed_mods_with_errors(&sb.path());
        assert!(
            report.all_installs.len() >= 2,
            "a corrupt profile must not hide the mods, got {}",
            report.all_installs.len()
        );
        assert!(
            report.errors.iter().any(|e| e.contains("inst_broken.json")),
            "the corrupt profile must be reported, got {:?}",
            report.errors
        );
    }
}

