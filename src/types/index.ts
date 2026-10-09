/**
 * Project Zomboid Mod Studio (PZ Mod Studio)
 * Core TypeScript Foundations & Type Definitions
 */

// ==========================================
// Module 1: Virtual Path Detector & Multi-Way (N-Way) Merger
// ==========================================

export type FileType = 'LUA' | 'SCRIPT_TXT';

export type ConflictStatus = 'AUTO_MERGED' | 'MANUAL_CONFLICT' | 'RESOLVED' | 'PENDING';

export interface CompetingModFile {
  mod_id: string;
  mod_name: string;
  absolute_path: string;
  content: string;
  is_selected?: boolean;
}

export interface VfsConflict {
  id: string;
  relative_path: string; // e.g. "media/lua/client/ISUI/ISInventoryPane.lua"
  file_type: FileType;
  /** Plain-language reason this file collides (populated by the Rust scanner). */
  cause?: string;
  /** Free-form severity label reported by the Rust scanner, e.g. "Warning". */
  severity?: string;
  start_line?: number; // e.g. 1
  end_line?: number;   // e.g. 20
  conflict_line?: number; // e.g. 5
  total_file_lines?: number; // e.g. 20
  base_content: string; // Vanilla file content
  competing_mods: CompetingModFile[];
  merged_output?: string;
  auto_ast_output?: string;
  is_identical_noise?: boolean;
  status: ConflictStatus;
  has_syntax_errors?: boolean;
  syntax_error_message?: string;
}

// ==========================================
// Module 2: JSON-Driven Polyfill Engine
// ==========================================

export type RuleCategory =
  | 'ARGUMENT_TYPE_WRAPPER'
  | 'SAFE_GLOBAL'
  | 'TRANSLATOR_FIX'
  | 'REQUIRE_REDIRECT'
  | 'CUSTOM_SHIM';

export type RuleSeverity = 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW';

export interface PolyfillRule {
  id: string;
  name: string;
  description: string;
  category: RuleCategory;
  severity: RuleSeverity;
  enabled: boolean;
  pattern: {
    type: string;
    target_function?: string;
    target_global?: string;
    target_method?: string;
    [key: string]: unknown;
  };
  action: {
    type: string;
    wrapper_template?: string;
    shim_code?: string;
    regex?: string;
    replacement?: string;
    mappings?: Record<string, string>;
  };
}

// ==========================================
// Module 3: Mod List & Workshop Inspector
// ==========================================

export interface ModInfo {
  mod_id: string;
  name: string;
  description?: string;
  workshop_id?: string;
  author?: string;
  version?: string;
  pzversion?: string;
  icon_path?: string;
  poster_url?: string;
  url?: string;
  dependencies: string[]; // require= directives
  load_mod_after?: string[]; // loadModAfter= directives (optional ordering hints)
  incompatible?: string[]; // incompatible= directives
  enabled: boolean;
  is_library?: boolean;
  is_map_mod?: boolean;
  is_packaged?: boolean;
  load_order_index: number;
}

export interface DependencyIssue {
  mod_id: string;
  missing_dependency_id: string;
  issue_type: 'MISSING' | 'WRONG_ORDER' | 'CIRCULAR';
  severity: 'WARNING' | 'ERROR';
  description: string;
}

/**
 * Cross-mod diagnostic reported by the Rust `scan_mod_diagnostics_cmd` command.
 * NOTE: the Rust structs carry no `serde(rename_all)`, so every field arrives
 * from IPC in snake_case exactly as declared here.
 */
/**
 * Severity as it arrives on the wire.
 *
 * NOTE: Rust `Severity` is `#[serde(rename_all = "UPPERCASE")]`, so the raw IPC
 * value is "ERROR" / "WARNING" / "INFO". `TauriService.scanModDiagnostics`
 * normalizes those to Title Case ("Error" / "Warning" / "Info") case-insensitively,
 * which is the form this type describes. Do not compare raw payloads against
 * Title Case strings.
 *
 * (The separate `VfsConflict.severity` string is a different source — the Rust
 * `classify_vfs_conflict` returns Title Case there — hence the normalization.)
 */
export type DiagnosticSeverity = 'Error' | 'Warning' | 'Info';

/** Known diagnostic kinds. Unknown future kinds still render (string fallback). */
export type DiagnosticKind =
  | 'MISSING_DEPENDENCY'
  | 'LOAD_ORDER_VIOLATION'
  | 'CIRCULAR_DEPENDENCY'
  | 'INCOMPATIBLE_PAIR'
  | 'DUPLICATE_MOD'
  | 'GAME_VERSION_MISMATCH'
  | 'REQUIRED_LIBRARY_VERSION'
  | 'MALFORMED_VERSION_DIRECTIVE'
  | 'DATA_KEY_COLLISION'
  | 'ASSET_COLLISION'
  | 'FILE_COLLISION'
  | (string & {});

export interface ModDiagnostic {
  kind: DiagnosticKind;
  severity: DiagnosticSeverity;
  title: string;
  cause: string;
  mod_ids: string[];
  related_mod_ids: string[];
  file_path: string | null;
  detail: string | null;
  suggestion: string | null;
}

export interface ModProfile {
  id: string;
  profile_name: string;
  created_at: string;
  pz_version: string;
  mod_count: number;
  load_order: string[]; // list of mod_ids
}

// ==========================================
// Module 4: Monitor Center & Crash Diagnostics
// ==========================================

export type SandboxStatus = 'IDLE' | 'BOOTING' | 'RUNNING' | 'SUCCESS' | 'CRASHED';

export type GameLaunchMode =
  | 'DEBUG_FULLSCREEN'
  | 'DEBUG_WINDOWED'
  | 'NORMAL_FULLSCREEN'
  | 'NORMAL_WINDOWED'
  | 'MONITORED'
  | 'WINDOWED'
  | 'NORMAL';

export interface TranslatedErrorCard {
  id: string;
  raw_error: string;
  source_file?: string;
  line_number?: number;
  title: string;
  explanation: string;
  recommended_action: string;
  polyfill_rule_id_suggestion?: string;
}

export interface SandboxSession {
  status: SandboxStatus;
  test_mode: 'BACKGROUND_QUICK' | 'WINDOWED_DEEP';
  start_time?: string;
  elapsed_seconds: number;
  log_lines: string[];
  error_cards: TranslatedErrorCard[];
}

export interface StudioPathsUI {
  pz_install_dir: string;
  workshop_dir: string;
  user_zomboid_dir: string;
  mod_list_ini_path: string;
  is_valid: boolean;
}

// ==========================================
// Module 5: Presets, Server Manager & Instances
// ==========================================

export interface PresetModEntry {
  mod_id: string;
  name: string;
  workshop_id?: string;
  enabled: boolean;
}

export interface ModPreset {
  id: string;
  name: string;
  description?: string;
  author?: string;
  created_at: string;
  mods: PresetModEntry[];
  load_order: string[];
}

export interface MissingModsReport {
  missing_mods: PresetModEntry[];
  installed_count: number;
  total_count: number;
}

export interface PZServerConfig {
  name: string;
  file_path: string;
  mods: string[];
  workshop_items: string[];
}

export interface DedicatedServerStatus {
  is_running: boolean;
  pid?: number | null;
  server_name?: string | null;
  memory_gb?: number | null;
  start_timestamp?: number | null;
}

export interface ConnectedPlayer {
  username: string;
  role: string;
  ping: number;
  health: number;
  is_godmode: boolean;
  x: number;
  y: number;
  z: number;
  steam_id: string;
}

export interface ServerQuickSettings {
  public_name: string;
  public_description: string;
  password: string;
  max_players: number;
  pvp: boolean;
  pause_empty: boolean;
  open: boolean;
  port: number;
  rcon_port: number;
  rcon_password: string;
  map_names: string;
}

export interface LogFileInfoUI {
  file_name: string;
  absolute_path: string;
  size_bytes: number;
  modified_timestamp: number;
  is_active_console: boolean;
}

export interface AppInstance {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  is_active: boolean;
  active_mod_ids: string[];
  load_order: string[];
}

export type AppProfile = AppInstance;

export type ActiveTab = 'MOD_LIST' | 'SERVERS' | 'PROFILES' | 'MERGER' | 'MONITOR' | 'SETTINGS';
