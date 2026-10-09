use super::protocol::{McpTool, ToolCallResult, ToolContent};
use crate::diff_engine::lua::{three_way_merge_lua, validate_lua_syntax};
use crate::load_order::mod_info::scan_all_installed_mods;
use crate::load_order::topological_sort::sort_dependencies_topologically;
use crate::instance_manager::{activate_instance, create_instance, list_instances};
use crate::patch_generator::{get_master_patch_status, list_merged_packages, save_draft_resolution};
use crate::sandbox::{
    find_running_pz_pid, install_bridge_companion_mod, kill_pz_game, launch_pz_direct,
    list_available_log_files, read_game_ipc_response, read_log_file, translate_log_error,
    write_game_ipc_command, TranslatedErrorPayload,
};
use crate::vfs::{auto_detect_paths, scan_conflicts, StudioPaths};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Returns all tool definitions exposed by the MCP server.
pub fn get_available_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "get_studio_paths".to_string(),
            description: "Auto-detects and returns Project Zomboid installation, Steam workshop, and user data directory paths.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "get_game_status".to_string(),
            description: "Checks if ProjectZomboid64.exe is currently running on the system and returns its Process ID (PID).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "launch_game".to_string(),
            description: "Launches ProjectZomboid64.exe process with configurable flags such as debug mode, windowed mode, and nosteam.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pz_install_dir": { "type": "string", "description": "Optional custom game installation path" },
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "debug_mode": { "type": "boolean", "description": "If true, launches with -debug flag (default: true)" },
                    "windowed": { "type": "boolean", "description": "If true, forces windowed execution with -windowed (default: true)" },
                    "nosteam": { "type": "boolean", "description": "If true, launches with -nosteam (default: false)" },
                    "extra_args": { "type": "array", "items": { "type": "string" }, "description": "Optional extra CLI arguments" }
                }
            }),
        },
        McpTool {
            name: "terminate_game".to_string(),
            description: "Terminates the running ProjectZomboid64.exe process cleanly or forcefully via OS process management.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "Optional specific Process ID (PID) to terminate. If omitted, terminates any ProjectZomboid64.exe" },
                    "force": { "type": "boolean", "description": "If true, forces immediate termination (/F flag) (default: true)" }
                }
            }),
        },
        McpTool {
            name: "send_game_ipc_command".to_string(),
            description: "Sends an in-game IPC command (e.g. give_item, set_godmode, eval_lua) to a live Project Zomboid session via the Z_PZModStudio_Bridge companion mod.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "command": {
                        "type": "object",
                        "description": "The command object to execute. Example: {\"action\": \"give_item\", \"item\": \"Base.Axe\", \"equip\": true} or {\"action\": \"eval_lua\", \"code\": \"getPlayer():Say('Hello!')\"} or {\"action\": \"set_godmode\"}"
                    }
                },
                "required": ["command"]
            }),
        },
        McpTool {
            name: "get_game_ipc_response".to_string(),
            description: "Reads the latest execution response emitted by the in-game companion bridge mod.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                }
            }),
        },
        McpTool {
            name: "install_bridge_companion_mod".to_string(),
            description: "Installs the Z_PZModStudio_Bridge companion mod into the user's Zomboid/mods directory to enable live in-game Lua execution and item equipping.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                }
            }),
        },
        McpTool {
            name: "get_monitor_logs".to_string(),
            description: "Reads recent lines from Project Zomboid's console.txt log file, with optional line limit and error filtering.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": {
                        "type": "string",
                        "description": "Optional custom path to the user Zomboid folder (e.g. C:\\Users\\Name\\Zomboid)"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum number of lines to retrieve from the end of console.txt (default: 100)"
                    },
                    "errors_only": {
                        "type": "boolean",
                        "description": "If true, only returns lines containing ERROR, Exception, Stacktrace, or Callframe"
                    }
                }
            }),
        },
        McpTool {
            name: "get_crash_diagnostics".to_string(),
            description: "Scans console.txt in real time to extract Lua and Java exceptions, stacktraces, and actionable repair cards for PZ Build 41 & 42.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": {
                        "type": "string",
                        "description": "Optional custom path to the user Zomboid folder"
                    }
                }
            }),
        },
        McpTool {
            name: "list_installed_mods".to_string(),
            description: "Scans and lists all installed Project Zomboid mods from Steam Workshop and local user mods folder, including mod IDs, names, versions, and compatibility tags.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pz_install_dir": { "type": "string", "description": "Optional custom game install directory" },
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "workshop_dir": { "type": "string", "description": "Optional custom workshop content directory (e.g. 108600)" }
                }
            }),
        },
        McpTool {
            name: "sort_mod_load_order".to_string(),
            description: "Performs topological dependency sorting on installed mods and reports missing dependencies or circular load-order conflicts.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pz_install_dir": { "type": "string" },
                    "user_zomboid_dir": { "type": "string" },
                    "workshop_dir": { "type": "string" }
                }
            }),
        },
        McpTool {
            name: "scan_mod_conflicts".to_string(),
            description: "Scans active mods for Virtual File System (VFS) conflicts where multiple mods overwrite the same Lua scripts or PZ data definitions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pz_install_dir": { "type": "string" },
                    "user_zomboid_dir": { "type": "string" },
                    "workshop_dir": { "type": "string" }
                }
            }),
        },
        McpTool {
            name: "scan_mod_diagnostics".to_string(),
            description: "Runs the plain-language conflict engine over every installed mod and returns user-readable diagnostics: missing dependencies, incompatible pairs, circular dependencies, duplicate installs, and unsafe version lines. Each item carries a severity, a human-readable cause, and a suggested fix.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pz_install_dir": { "type": "string" },
                    "user_zomboid_dir": { "type": "string" },
                    "workshop_dir": { "type": "string" }
                }
            }),
        },
        McpTool {
            name: "validate_lua_syntax".to_string(),
            description: "Validates Lua code syntax using full_moon AST parser, returning precise line, column, and syntax error messages if invalid.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "The raw Lua code to validate" }
                },
                "required": ["code"]
            }),
        },
        McpTool {
            name: "merge_lua_scripts".to_string(),
            description: "Performs a 3-way AST merge on Lua scripts: Base (vanilla), Target A (Mod A), and Target B (Mod B), producing a unified, conflict-free script.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "base": { "type": "string", "description": "Vanilla base Lua script" },
                    "target_a": { "type": "string", "description": "Variant A Lua script" },
                    "target_b": { "type": "string", "description": "Variant B Lua script" }
                },
                "required": ["base", "target_a", "target_b"]
            }),
        },
        McpTool {
            name: "get_master_patch_status".to_string(),
            description: "Queries the current status of Z_PZModStudio_MasterPatch package, including generated patches and draft resolutions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "package_folder_name": { "type": "string", "description": "Package folder name (default: MasterPatch)" }
                }
            }),
        },
        McpTool {
            name: "save_draft_resolution".to_string(),
            description: "Saves a resolved Lua script or PZ data definition directly into the active Master Patch draft resolutions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "package_folder_name": { "type": "string", "description": "Package folder name (default: MasterPatch)" },
                    "relative_path": { "type": "string", "description": "Relative file path inside the mod (e.g. media/lua/shared/ISInventoryPane.lua)" },
                    "resolved_content": { "type": "string", "description": "The merged or patched file content" },
                    "status": { "type": "string", "description": "Resolution status: MANUAL, AST_AUTO, or AI_RESOLVED (default: AI_RESOLVED)" }
                },
                "required": ["relative_path", "resolved_content"]
            }),
        },
        McpTool {
            name: "list_mod_profiles".to_string(),
            description: "Lists all saved mod profiles/instances, their enabled mod IDs, custom load orders, and active status.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                }
            }),
        },
        McpTool {
            name: "create_mod_profile".to_string(),
            description: "Creates a new named mod profile with specific active mod IDs and load order.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Profile name (e.g. Brita Overhaul B42)" },
                    "description": { "type": "string", "description": "Optional profile description" },
                    "active_mod_ids": { "type": "array", "items": { "type": "string" }, "description": "List of active mod IDs" },
                    "load_order": { "type": "array", "items": { "type": "string" }, "description": "List of mod IDs in load order" },
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                },
                "required": ["name"]
            }),
        },
        McpTool {
            name: "activate_mod_profile".to_string(),
            description: "Activates a saved mod profile by ID, writing its active mods and load order directly to default.txt / ModListData.ini.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_id": { "type": "string", "description": "The unique ID of the profile to activate (e.g. inst_1787112345)" },
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                },
                "required": ["profile_id"]
            }),
        },
        McpTool {
            name: "list_available_logs".to_string(),
            description: "Discovers and lists all Project Zomboid log files on disk (console.txt and timestamped session logs in Zomboid/Logs/).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" }
                }
            }),
        },
        McpTool {
            name: "read_log_file".to_string(),
            description: "Reads lines from any specific log file on disk with optional line limits and error filtering.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Absolute path to the log file to read" },
                    "max_lines": { "type": "integer", "description": "Maximum number of lines to read from the end (default: 200)" },
                    "errors_only": { "type": "boolean", "description": "If true, only returns lines with errors or exceptions" }
                },
                "required": ["file_path"]
            }),
        },
        McpTool {
            name: "list_merged_packages".to_string(),
            description: "Lists all fusion packages (Z_PZModStudio_*), their active state in mod list, and packaged files.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "user_zomboid_dir": { "type": "string", "description": "Optional custom user Zomboid directory" },
                    "mod_list_ini_path": { "type": "string", "description": "Optional path to default.txt or ModListData.ini" }
                }
            }),
        },
    ]
}

/// Dispatches a tool execution request by name.
pub fn execute_tool(name: &str, args: Value) -> ToolCallResult {
    match name {
        "get_studio_paths" => handle_get_studio_paths(),
        "get_game_status" => handle_get_game_status(),
        "launch_game" => handle_launch_game(args),
        "terminate_game" => handle_terminate_game(args),
        "send_game_ipc_command" => handle_send_game_ipc_command(args),
        "get_game_ipc_response" => handle_get_game_ipc_response(args),
        "install_bridge_companion_mod" => handle_install_bridge_companion_mod(args),
        "get_monitor_logs" => handle_get_monitor_logs(args),
        "get_crash_diagnostics" => handle_get_crash_diagnostics(args),
        "list_installed_mods" => handle_list_installed_mods(args),
        "sort_mod_load_order" => handle_sort_mod_load_order(args),
        "scan_mod_conflicts" => handle_scan_mod_conflicts(args),
        "scan_mod_diagnostics" => handle_scan_mod_diagnostics(args),
        "validate_lua_syntax" => handle_validate_lua_syntax(args),
        "merge_lua_scripts" => handle_merge_lua_scripts(args),
        "get_master_patch_status" => handle_get_master_patch_status(args),
        "save_draft_resolution" => handle_save_draft_resolution(args),
        "list_mod_profiles" => handle_list_mod_profiles(args),
        "create_mod_profile" => handle_create_mod_profile(args),
        "activate_mod_profile" => handle_activate_mod_profile(args),
        "list_available_logs" => handle_list_available_logs(args),
        "read_log_file" => handle_read_log_file(args),
        "list_merged_packages" => handle_list_merged_packages(args),
        _ => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Unknown tool: '{}'", name),
            }],
            is_error: true,
        },
    }
}

fn resolve_paths(args: &Value) -> StudioPaths {
    let mut paths = auto_detect_paths();
    if let Some(pz) = args.get("pz_install_dir").and_then(|v| v.as_str()) {
        if !pz.is_empty() {
            paths.pz_install_dir = pz.to_string();
        }
    }
    if let Some(uz) = args.get("user_zomboid_dir").and_then(|v| v.as_str()) {
        if !uz.is_empty() {
            paths.user_zomboid_dir = uz.to_string();
            paths.mod_list_ini_path = Path::new(uz)
                .join("mods")
                .join("ModListData.ini")
                .to_string_lossy()
                .to_string();
        }
    }
    if let Some(ws) = args.get("workshop_dir").and_then(|v| v.as_str()) {
        if !ws.is_empty() {
            paths.workshop_dir = ws.to_string();
        }
    }
    paths
}

fn handle_get_studio_paths() -> ToolCallResult {
    let paths = auto_detect_paths();
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&paths).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_get_game_status() -> ToolCallResult {
    let pid_opt = find_running_pz_pid();
    let result = json!({
        "is_running": pid_opt.is_some(),
        "process_id": pid_opt,
        "process_name": "ProjectZomboid64.exe",
        "description": if pid_opt.is_some() {
            "Project Zomboid is actively running."
        } else {
            "Project Zomboid is not running."
        }
    });

    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_get_monitor_logs(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let max_lines = args.get("max_lines").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    let errors_only = args.get("errors_only").and_then(|v| v.as_bool()).unwrap_or(false);

    let console_path = Path::new(&paths.user_zomboid_dir).join("console.txt");
    if !console_path.exists() {
        return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("console.txt log file not found at: {}", console_path.display()),
            }],
            is_error: true,
        };
    }

    match File::open(&console_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut collected = Vec::new();

            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    if errors_only {
                        let is_err = line.contains("ERROR")
                            || line.contains("Exception")
                            || line.contains("Kahlua")
                            || line.contains("Callframe:")
                            || line.contains("Stacktrace:")
                            || line.contains("at zombie.");
                        if is_err {
                            collected.push(line);
                        }
                    } else {
                        collected.push(line);
                    }
                }
            }

            let start = if collected.len() > max_lines {
                collected.len() - max_lines
            } else {
                0
            };
            let slice = &collected[start..];

            let result = json!({
                "log_file": console_path.to_string_lossy(),
                "total_matched_lines": collected.len(),
                "returned_lines_count": slice.len(),
                "lines": slice
            });

            ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {}", e)),
                }],
                is_error: false,
            }
        }
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to read console.txt: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_get_crash_diagnostics(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let console_path = Path::new(&paths.user_zomboid_dir).join("console.txt");

    if !console_path.exists() {
        return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("console.txt not found at: {}", console_path.display()),
            }],
            is_error: true,
        };
    }

    match File::open(&console_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut error_cards: Vec<TranslatedErrorPayload> = Vec::new();
            let mut raw_error_snippets: Vec<String> = Vec::new();
            let mut counter = 1;

            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    if let Some(card) = translate_log_error(&line, counter) {
                        error_cards.push(card);
                        counter += 1;
                    } else if line.contains("Exception") || line.contains("ERROR:") {
                        if raw_error_snippets.len() < 30 {
                            raw_error_snippets.push(line);
                        }
                    }
                }
            }

            let pid_opt = find_running_pz_pid();
            let result = json!({
                "game_running": pid_opt.is_some(),
                "pid": pid_opt,
                "diagnosed_error_cards_count": error_cards.len(),
                "diagnosed_error_cards": error_cards,
                "raw_error_sample_lines": raw_error_snippets
            });

            ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {}", e)),
                }],
                is_error: false,
            }
        }
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to read console.txt: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_list_installed_mods(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let manifests = scan_all_installed_mods(&paths);

    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&json!({
                "total_installed_mods": manifests.len(),
                "mods": manifests
            }))
            .unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_sort_mod_load_order(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let manifests = scan_all_installed_mods(&paths);
    let analysis = sort_dependencies_topologically(&manifests);

    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&analysis).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_scan_mod_conflicts(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let conflicts = scan_conflicts(&paths);

    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&json!({
                "total_vfs_conflicts": conflicts.len(),
                "conflicts": conflicts
            }))
            .unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_scan_mod_diagnostics(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    // Graceful scan: a mod we cannot read is reported in `scan_errors` instead of
    // aborting the whole scan, so one broken `mod.info` cannot hide every other
    // mod from an agent.
    let report = crate::load_order::mod_info::scan_all_installed_mods_with_errors(&paths);
    let rules = crate::compat::global_rules();
    let diagnostics = crate::conflicts::analyze_with_compat(&report.all_installs, rules, None);

    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == crate::conflicts::Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == crate::conflicts::Severity::Warning)
        .count();
    let infos = diagnostics.len() - errors - warnings;

    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&json!({
                "total_mods_scanned": report.all_installs.len(),
                "errors": errors,
                "warnings": warnings,
                "infos": infos,
                "diagnostics": diagnostics,
                // NEW: files/folders we could not read. Distinct from
                // `errors`, which counts mod problems.
                "scan_errors": report.errors,
                // NEW: where the compatibility catalog came from. An agent must
                // check this before reporting "no known incompatibilities".
                "compatibility_status": rules.status,
                "compatibility_data_available": rules.is_available()
            }))
            .unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_validate_lua_syntax(args: Value) -> ToolCallResult {
    let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if code.is_empty() {
        return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Missing or empty 'code' argument to validate_lua_syntax.".to_string(),
            }],
            is_error: true,
        };
    }

    let check = validate_lua_syntax(code);
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&check).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_merge_lua_scripts(args: Value) -> ToolCallResult {
    let base = args.get("base").and_then(|v| v.as_str()).unwrap_or("");
    let target_a = args.get("target_a").and_then(|v| v.as_str()).unwrap_or("");
    let target_b = args.get("target_b").and_then(|v| v.as_str()).unwrap_or("");

    let res = three_way_merge_lua(base, target_a, target_b);
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&res).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_get_master_patch_status(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let pkg_name = args.get("package_folder_name").and_then(|v| v.as_str()).map(|s| s.to_string());

    let status = get_master_patch_status(&paths.user_zomboid_dir, &paths.mod_list_ini_path, pkg_name);
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&status).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_save_draft_resolution(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let pkg_name = args.get("package_folder_name").and_then(|v| v.as_str()).unwrap_or("MasterPatch");
    let relative_path = args.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");
    let resolved_content = args.get("resolved_content").and_then(|v| v.as_str()).unwrap_or("");
    let status = args.get("status").and_then(|v| v.as_str()).unwrap_or("AI_RESOLVED");

    if relative_path.is_empty() || resolved_content.is_empty() {
        return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Both 'relative_path' and 'resolved_content' are required arguments.".to_string(),
            }],
            is_error: true,
        };
    }

    match save_draft_resolution(
        &paths.user_zomboid_dir,
        pkg_name,
        relative_path,
        resolved_content,
        status,
    ) {
        Ok(ok) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "success": ok,
                    "relative_path": relative_path,
                    "package": pkg_name,
                    "status": status,
                    "message": "Draft resolution successfully saved to Master Patch folder."
                }))
                .unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to save draft resolution: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_launch_game(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let debug_mode = args.get("debug_mode").and_then(|v| v.as_bool()).unwrap_or(true);
    let windowed = args.get("windowed").and_then(|v| v.as_bool()).unwrap_or(true);
    let nosteam = args.get("nosteam").and_then(|v| v.as_bool()).unwrap_or(false);
    let extra_args = args.get("extra_args").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<String>>()
    }).unwrap_or_default();

    match launch_pz_direct(&paths.pz_install_dir, &paths.user_zomboid_dir, debug_mode, windowed, nosteam, &extra_args) {
        Ok(pid) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "status": "launched",
                    "pid": pid,
                    "debug_mode": debug_mode,
                    "windowed": windowed,
                    "nosteam": nosteam,
                    "message": format!("Project Zomboid launched successfully with PID: {}", pid)
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to launch Project Zomboid: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_terminate_game(args: Value) -> ToolCallResult {
    let pid = args.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(true);

    match kill_pz_game(pid, force) {
        Ok(msg) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "status": "terminated",
                    "pid": pid,
                    "force": force,
                    "message": msg
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to terminate game process: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_send_game_ipc_command(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let command = match args.get("command") {
        Some(cmd) => cmd.clone(),
        None => return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Missing required argument 'command'.".to_string(),
            }],
            is_error: true,
        },
    };

    match write_game_ipc_command(&paths.user_zomboid_dir, command) {
        Ok(msg) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "status": "enqueued",
                    "message": msg
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to enqueue IPC command: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_get_game_ipc_response(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);

    match read_game_ipc_response(&paths.user_zomboid_dir) {
        Ok(resp) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "has_response": resp.is_some(),
                    "response": resp
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to read IPC response: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_install_bridge_companion_mod(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);

    match install_bridge_companion_mod(&paths.user_zomboid_dir) {
        Ok(msg) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "status": "installed",
                    "message": msg
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to install bridge companion mod: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_list_mod_profiles(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    match list_instances(paths.user_zomboid_dir) {
        Ok(list) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "total_profiles": list.len(),
                    "profiles": list
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to list mod profiles: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_create_mod_profile(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Missing required argument 'name'.".to_string(),
            }],
            is_error: true,
        },
    };

    let description = args.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
    let active_mod_ids: Vec<String> = args.get("active_mod_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let load_order: Vec<String> = args.get("load_order")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    match create_instance(paths.user_zomboid_dir, name, description, active_mod_ids, load_order) {
        Ok(inst) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&inst).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to create mod profile: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_activate_mod_profile(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let profile_id = match args.get("profile_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Missing required argument 'profile_id'.".to_string(),
            }],
            is_error: true,
        },
    };

    match activate_instance(paths.user_zomboid_dir, profile_id) {
        Ok(_) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&json!({
                    "status": "activated",
                    "message": "Mod profile activated and written to default.txt / ModListData.ini"
                })).unwrap_or_else(|e| format!("Error: {}", e)),
            }],
            is_error: false,
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to activate mod profile: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_list_available_logs(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let list = list_available_log_files(&paths.user_zomboid_dir);
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&json!({
                "total_logs": list.len(),
                "logs": list
            })).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

fn handle_read_log_file(args: Value) -> ToolCallResult {
    let file_path = match args.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: "Missing required argument 'file_path'.".to_string(),
            }],
            is_error: true,
        },
    };

    let max_lines = args.get("max_lines").and_then(|v| v.as_u64()).map(|n| n as usize).or(Some(200));
    let errors_only = args.get("errors_only").and_then(|v| v.as_bool()).unwrap_or(false);

    match read_log_file(file_path, max_lines) {
        Ok(lines) => {
            let final_lines: Vec<String> = if errors_only {
                lines.into_iter().filter(|l| {
                    let low = l.to_lowercase();
                    low.contains("error") || low.contains("exception") || low.contains("crash") || low.contains("stacktrace")
                }).collect()
            } else {
                lines
            };

            ToolCallResult {
                content: vec![ToolContent {
                    content_type: "text".to_string(),
                    text: serde_json::to_string_pretty(&json!({
                        "file_path": file_path,
                        "returned_lines": final_lines.len(),
                        "lines": final_lines
                    })).unwrap_or_else(|e| format!("Error: {}", e)),
                }],
                is_error: false,
            }
        },
        Err(e) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: format!("Failed to read log file: {}", e),
            }],
            is_error: true,
        },
    }
}

fn handle_list_merged_packages(args: Value) -> ToolCallResult {
    let paths = resolve_paths(&args);
    let list = list_merged_packages(&paths.user_zomboid_dir, &paths.mod_list_ini_path);
    ToolCallResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&json!({
                "total_packages": list.len(),
                "packages": list
            })).unwrap_or_else(|e| format!("Error: {}", e)),
        }],
        is_error: false,
    }
}

