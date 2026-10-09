import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { StudioPathsUI } from '../components/settings/SettingsModule';
import {
  VfsConflict,
  ModInfo,
  TranslatedErrorCard,
  DedicatedServerStatus,
  ConnectedPlayer,
  ServerQuickSettings,
  PZServerConfig,
  LogFileInfoUI,
  ModDiagnostic,
  DiagnosticSeverity,
  FixPlan,
  FixKind,
  ApplyResult,
  BackupEntry,
  RestoreResult,
} from '../types';

export interface LuaSyntaxResult {
  is_valid: boolean;
  error_message?: string;
}

export interface MergeChunkResult {
  merged_text: string;
  has_conflict: boolean;
  conflict_line_start?: number;
  conflict_line_end?: number;
}

export interface ModListDataRaw {
  active_mods: string[];
  raw_ini_content: string;
}

export interface DependencyAnalysisRaw {
  sorted_mod_ids: string[];
  missing_dependencies: string[];
  has_circular_dependency: boolean;
}

export interface SandboxLogPayload {
  line: string;
  is_error: boolean;
}

export interface MasterPatchResultUI {
  success: boolean;
  patch_mod_dir: string;
  files_written: number;
  polyfills_injected: number;
}

export interface MasterPatchStatusInfoUI {
  is_packaged: boolean;
  created_at?: string;
  packaged_mods: string[];
  merged_files: string[];
  missing_active_mods: string[];
}

export interface MergedPackageInfoUI {
  folder_name: string;
  display_name: string;
  mod_id: string;
  description?: string;
  is_packaged: boolean;
  is_active_in_modlist: boolean;
  is_companion_bridge: boolean;
  created_at?: string;
  packaged_mods: string[];
  merged_files: string[];
}

const STORAGE_KEY_PATHS = 'pz_mod_studio_paths_profile';

/**
 * Service layer wrapping all native Tauri Rust backend IPC calls
 */
export const TauriService = {
  /**
   * Opens any external web URL in the user's default system browser
   */
  openExternalUrl: async (url: string): Promise<void> => {
    try {
      await openUrl(url);
      return;
    } catch (pluginErr) {
      console.warn('Tauri openUrl plugin fallback:', pluginErr);
    }

    try {
      await invoke('open_external_url_cmd', { url });
      return;
    } catch (ipcErr) {
      console.warn('Tauri open_external_url_cmd fallback:', ipcErr);
    }

    try {
      window.open(url, '_blank');
    } catch (winErr) {
      console.error('Failed to open external URL:', winErr);
    }
  },

  /**
   * Loads saved directory paths profile from LocalStorage or Rust auto-detection
   */
  loadSavedPathsProfile: async (): Promise<StudioPathsUI> => {
    try {
      const cached = localStorage.getItem(STORAGE_KEY_PATHS);
      if (cached) {
        const parsed = JSON.parse(cached) as StudioPathsUI;
        if (parsed.pz_install_dir) {
          // Re-validate cached paths with Rust
          return await invoke<StudioPathsUI>('set_and_validate_paths', { customPaths: parsed });
        }
      }
    } catch (err) {
      console.warn('Failed to load cached paths profile:', err);
    }
    // Fallback to Rust auto-detection
    return await TauriService.getAutoPaths();
  },

  /**
   * Saves directory paths profile persistently to LocalStorage
   */
  savePathsProfile: async (paths: StudioPathsUI): Promise<StudioPathsUI> => {
    const validated = await TauriService.validatePaths(paths);
    localStorage.setItem(STORAGE_KEY_PATHS, JSON.stringify(validated));
    return validated;
  },

  /**
   * Auto-detects Project Zomboid installation and user directories
   */
  getAutoPaths: async (): Promise<StudioPathsUI> => {
    try {
      return await invoke<StudioPathsUI>('get_auto_paths');
    } catch (err) {
      console.warn('Fallback to default paths:', err);
      return {
        pz_install_dir: '',
        workshop_dir: '',
        user_zomboid_dir: '',
        mod_list_ini_path: '',
        is_valid: false,
      };
    }
  },

  /**
   * Validates custom or manually entered directory paths
   */
  validatePaths: async (customPaths: StudioPathsUI): Promise<StudioPathsUI> => {
    try {
      return await invoke<StudioPathsUI>('set_and_validate_paths', { customPaths });
    } catch (err) {
      console.error('Failed to validate paths:', err);
      return customPaths;
    }
  },

  /**
   * Opens native Windows Explorer folder picker dialog
   */
  pickFolder: async (defaultPath?: string): Promise<string | null> => {
    try {
      return await invoke<string | null>('pick_folder_cmd', { defaultPath });
    } catch (err) {
      console.error('Folder picker error:', err);
      return null;
    }
  },

  /**
   * Scans Virtual File System for real conflicts between active mods
   */
  scanConflicts: async (paths: StudioPathsUI): Promise<VfsConflict[]> => {
    try {
      const rawConflicts = await invoke<any[]>('scan_conflicts_cmd', { paths });
      if (!rawConflicts || rawConflicts.length === 0) {
        return [];
      }
      return rawConflicts.map((c) => ({
        id: c.id,
        relative_path: c.relative_path,
        file_type: c.file_type,
        cause: c.cause,
        severity: c.severity,
        start_line: c.start_line ?? 1,
        end_line: c.end_line ?? 10,
        conflict_line: c.conflict_line ?? 1,
        total_file_lines: c.total_file_lines ?? c.base_content.split('\n').length ?? 1,
        base_content: c.base_content,
        merged_output: c.merged_output || c.base_content,
        auto_ast_output: c.auto_ast_output || c.merged_output || c.base_content,
        is_identical_noise: c.is_identical_noise ?? false,
        competing_mods: c.competing_files.map((f: any) => ({
          mod_id: f.mod_id,
          mod_name: f.mod_name,
          absolute_path: f.absolute_path,
          content: f.content,
        })),
        status: (c.status as any) || (c.is_identical_noise ? ('AUTO_MERGED' as const) : ('MANUAL_CONFLICT' as const)),
      }));
    } catch (err) {
      console.warn('Scan conflicts error:', err);
      return [];
    }
  },

  /**
   * Scans all subscribed Workshop & local mods, ordering active ones first according to ModListData.ini
   */
  scanAllInstalledMods: async (paths: StudioPathsUI): Promise<ModInfo[]> => {
    try {
      const rawMods = await invoke<any[]>('scan_all_installed_mods_cmd', { paths });
      if (!rawMods || rawMods.length === 0) {
        return [];
      }
      return rawMods.map((m, idx) => ({
        mod_id: m.id,
        name: m.name || m.id,
        description: m.description,
        workshop_id: m.workshop_id,
        author: m.author,
        version: m.version,
        pzversion: m.pzversion,
        url: m.url,
        dependencies: m.require || [],
        load_mod_after: m.load_mod_after || [],
        incompatible: m.incompatible || [],
        enabled: m.enabled ?? false,
        icon_path: m.icon_path,
        poster_url: m.poster_url,
        is_library: m.is_library ?? false,
        is_map_mod: m.is_map_mod ?? false,
        is_packaged: m.is_packaged,
        load_order_index: idx + 1,
      }));
    } catch (err) {
      console.warn('Failed to scan installed mods:', err);
      return [];
    }
  },

  /**
   * Validates Lua AST syntax using Rust full_moon parser
   */
  validateLuaSyntax: async (code: string): Promise<LuaSyntaxResult> => {
    try {
      return await invoke<LuaSyntaxResult>('validate_lua_syntax_cmd', { code });
    } catch (err) {
      return { is_valid: false, error_message: String(err) };
    }
  },

  /**
   * Performs 3-way AST merge on Lua code
   */
  threeWayMergeLua: async (base: string, targetA: string, targetB: string): Promise<MergeChunkResult> => {
    try {
      return await invoke<MergeChunkResult>('three_way_merge_lua_cmd', { base, targetA, targetB });
    } catch (err) {
      return { merged_text: base, has_conflict: true };
    }
  },

  /**
   * Reads ModListData.ini file
   */
  readModListIni: async (iniPath: string): Promise<ModListDataRaw> => {
    try {
      return await invoke<ModListDataRaw>('read_mod_list_ini_cmd', { iniPath });
    } catch (err) {
      return { active_mods: [], raw_ini_content: '' };
    }
  },

  /**
   * Writes active mod load order back to ModListData.ini
   */
  writeModListIni: async (iniPath: string, activeMods: string[]): Promise<void> => {
    try {
      await invoke('write_mod_list_ini_cmd', { iniPath, activeMods });
    } catch (err) {
      console.error('Failed to write ModListData.ini:', err);
    }
  },

  /**
   * Generates the synthetic master patch mod under Zomboid/mods/Z_PZModStudio_MergedPatch
   */
  generateMasterPatch: async (req: {
    workshop_dir?: string;
    pz_install_dir?: string;
    user_zomboid_dir: string;
    mod_list_ini_path: string;
    merged_files: { relative_path: string; content: string }[];
    active_polyfill_ids: string[];
    package_folder_name?: string;
  }): Promise<MasterPatchResultUI> => {
    try {
      return await invoke<MasterPatchResultUI>('generate_master_patch_cmd', { req });
    } catch (err) {
      console.error('Failed to generate master patch:', err);
      return { success: false, patch_mod_dir: '', files_written: 0, polyfills_injected: 0 };
    }
  },

  /**
   * Safely removes Z_PZModStudio_MergedPatch synthetic patch files from disk
   */
  cleanMasterPatch: async (req: {
    workshop_dir?: string;
    pz_install_dir?: string;
    user_zomboid_dir: string;
    mod_list_ini_path: string;
    merged_files?: { relative_path: string; content: string }[];
    active_polyfill_ids?: string[];
    package_folder_name?: string;
  }): Promise<boolean> => {
    try {
      return await invoke<boolean>('clean_master_patch_cmd', {
        req: {
          ...req,
          merged_files: req.merged_files || [],
          active_polyfill_ids: req.active_polyfill_ids || [],
        },
      });
    } catch (err) {
      console.error('Failed to clean master patch:', err);
      return false;
    }
  },

  /**
   * Reads current packaging status and metadata for Z_PZModStudio_MergedPatch
   */
  getMasterPatchStatus: async (userZomboidDir: string, modListIniPath: string, packageFolderName?: string): Promise<MasterPatchStatusInfoUI> => {
    try {
      return await invoke<MasterPatchStatusInfoUI>('get_master_patch_status_cmd', {
        userZomboidDir,
        modListIniPath,
        packageFolderName: packageFolderName || null,
      });
    } catch (err) {
      console.error('Failed to get master patch status:', err);
      return {
        is_packaged: false,
        packaged_mods: [],
        merged_files: [],
        missing_active_mods: [],
      };
    }
  },

  /**
   * Lists all existing merged packages under Zomboid/mods/Z_PZModStudio_*
   */
  listMergedPackages: async (userZomboidDir: string, modListIniPath: string): Promise<MergedPackageInfoUI[]> => {
    try {
      return await invoke<MergedPackageInfoUI[]>('list_merged_packages_cmd', { userZomboidDir, modListIniPath });
    } catch (err) {
      console.error('Failed to list merged packages:', err);
      return [];
    }
  },

  /**
   * Creates a new named merged package (e.g. "Mix Mods" -> Z_PZModStudio_MixMods)
   */
  createMergedPackage: async (userZomboidDir: string, modListIniPath: string, name: string, description?: string): Promise<MergedPackageInfoUI | null> => {
    try {
      return await invoke<MergedPackageInfoUI>('create_merged_package_cmd', { userZomboidDir, modListIniPath, name, description });
    } catch (err) {
      console.error('Failed to create merged package:', err);
      return null;
    }
  },

  /**
   * Renames an existing merged package
   */
  renameMergedPackage: async (userZomboidDir: string, modListIniPath: string, oldFolder: string, newName: string, description?: string): Promise<MergedPackageInfoUI | null> => {
    try {
      return await invoke<MergedPackageInfoUI>('rename_merged_package_cmd', { userZomboidDir, modListIniPath, oldFolder, newName, description });
    } catch (err) {
      console.error('Failed to rename merged package:', err);
      return null;
    }
  },

  /**
   * Toggles a merged package presence (active/hidden) in ModListData.ini / default.txt
   */
  togglePackageInModlist: async (
    userZomboidDir: string,
    modListIniPath: string,
    folderName: string,
    enabled: boolean
  ): Promise<boolean> => {
    try {
      return await invoke<boolean>('toggle_package_in_modlist_cmd', {
        userZomboidDir,
        modListIniPath,
        folderName,
        enabled,
      });
    } catch (err) {
      console.error('Failed to toggle package in modlist:', err);
      return false;
    }
  },

  /**
   * Deletes a merged package directory from disk
   */
  deleteMergedPackage: async (userZomboidDir: string, modListIniPath: string, folderName: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('delete_merged_package_cmd', {
        userZomboidDir,
        modListIniPath,
        folderName,
      });
    } catch (err) {
      console.error('Failed to delete merged package:', err);
      return false;
    }
  },

  saveDraftResolution: async (
    userZomboidDir: string,
    packageFolderName: string,
    relativePath: string,
    resolvedContent: string,
    status: string
  ): Promise<boolean> => {
    try {
      return await invoke<boolean>('save_draft_resolution_cmd', {
        userZomboidDir,
        packageFolderName,
        relativePath,
        resolvedContent,
        status,
      });
    } catch (err) {
      console.error('Failed to save draft resolution:', err);
      return false;
    }
  },

  getDraftResolutions: async (
    userZomboidDir: string,
    packageFolderName: string
  ): Promise<Record<string, { relative_path: string; resolved_content: string; status: string }>> => {
    try {
      return await invoke<Record<string, { relative_path: string; resolved_content: string; status: string }>>(
        'get_draft_resolutions_cmd',
        {
          userZomboidDir,
          packageFolderName,
        }
      );
    } catch (err) {
      console.error('Failed to get draft resolutions:', err);
      return {};
    }
  },

  clearDraftResolutions: async (
    userZomboidDir: string,
    packageFolderName: string
  ): Promise<boolean> => {
    try {
      return await invoke<boolean>('clear_draft_resolutions_cmd', {
        userZomboidDir,
        packageFolderName,
      });
    } catch (err) {
      console.error('Failed to clear draft resolutions:', err);
      return false;
    }
  },
  exportMergedPackage: async (
    userZomboidDir: string,
    packageFolderName: string,
    targetFilePath: string
  ): Promise<boolean> => {
    try {
      return await invoke<boolean>('export_merged_package_cmd', {
        userZomboidDir,
        packageFolderName,
        targetFilePath,
      });
    } catch (err) {
      console.error('Failed to export merged package:', err);
      throw err;
    }
  },

  importMergedPackage: async (
    userZomboidDir: string,
    modListIniPath: string,
    sourceFilePath: string
  ): Promise<MergedPackageInfoUI> => {
    try {
      return await invoke<MergedPackageInfoUI>('import_merged_package_cmd', {
        userZomboidDir,
        modListIniPath,
        sourceFilePath,
      });
    } catch (err) {
      console.error('Failed to import merged package:', err);
      throw err;
    }
  },

  /**
   * Spawns isolated sandbox test run
   */
  launchSandbox: async (config: { pz_install_dir: string; user_zomboid_dir: string; test_mode: string }): Promise<number> => {
    try {
      return await invoke<number>('launch_sandbox_cmd', { config });
    } catch (err) {
      console.error('Sandbox launch failed:', err);
      return 0;
    }
  },

  /**
   * Listens to realtime log streaming events from Rust sandbox
   */
  listenSandboxLogs: async (callback: (payload: SandboxLogPayload) => void): Promise<UnlistenFn> => {
    return await listen<SandboxLogPayload>('sandbox-log', (event) => {
      callback(event.payload);
    });
  },

  /**
   * Listens to translated crash error cards from Rust log watcher
   */
  /**
   * Listens to translated crash error cards from Rust log watcher
   */
  listenSandboxErrorCards: async (callback: (card: TranslatedErrorCard) => void): Promise<UnlistenFn> => {
    return await listen<TranslatedErrorCard>('sandbox-error-card', (event) => {
      callback(event.payload);
    });
  },



  // ==========================================
  // Presets & File Dialogs (.pzpack, .pzmerge)
  // ==========================================
  pickSaveFile: async (defaultName?: string, filterName?: string, filterExt?: string): Promise<string | null> => {
    try {
      return await invoke<string | null>('pick_save_file_cmd', {
        defaultName,
        filterName,
        filterExt,
      });
    } catch (err) {
      return null;
    }
  },

  pickOpenFile: async (filterName?: string, filterExt?: string): Promise<string | null> => {
    try {
      return await invoke<string | null>('pick_open_file_cmd', {
        filterName,
        filterExt,
      });
    } catch (err) {
      return null;
    }
  },

  openPackageFolder: async (userZomboidDir: string, packageFolderName: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('open_package_folder_cmd', {
        userZomboidDir,
        packageFolderName,
      });
    } catch (err) {
      console.error('Failed to open package folder in explorer:', err);
      return false;
    }
  },

  openLogsFolder: async (userZomboidDir: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('open_logs_folder_cmd', { userZomboidDir });
    } catch (err) {
      console.error('Failed to open logs folder in explorer:', err);
      return false;
    }
  },

  listAvailableLogFiles: async (userZomboidDir: string): Promise<any[]> => {
    try {
      return await invoke<any[]>('list_available_log_files_cmd', { userZomboidDir });
    } catch (err) {
      console.error('Failed to list available log files:', err);
      return [];
    }
  },

  readLogFile: async (filePath: string, maxLines?: number): Promise<string[]> => {
    try {
      return await invoke<string[]>('read_log_file_cmd', { filePath, maxLines });
    } catch (err) {
      console.error('Failed to read log file:', err);
      return [];
    }
  },

  /**
   * Strict variants of the two log readers above. The plain wrappers swallow
   * transport errors and return `[]`, which is right for a monitor panel but
   * wrong for the diagnostic report: an unreadable log would be reported as
   * "no log files found", i.e. a false all-clear in the one artifact a user
   * attaches to a bug report. These rethrow so the caller can record the
   * failure honestly.
   */
  listAvailableLogFilesStrict: async (userZomboidDir: string): Promise<LogFileInfoUI[]> => {
    return await invoke<LogFileInfoUI[]>('list_available_log_files_cmd', { userZomboidDir });
  },

  readLogFileStrict: async (filePath: string, maxLines?: number): Promise<string[]> => {
    return await invoke<string[]>('read_log_file_cmd', { filePath, maxLines });
  },

  exportPresetFile: async (preset: any, filePath: string): Promise<void> => {
    return await invoke('export_preset_file', { preset, filePath });
  },

  importPresetFile: async (filePath: string): Promise<any> => {
    return await invoke('import_preset_file', { filePath });
  },

  checkMissingPresetMods: async (preset: any, userZomboidDir: string, workshopDir: string): Promise<any> => {
    return await invoke('check_missing_preset_mods', { preset, userZomboidDir, workshopDir });
  },

  // ==========================================
  // Server Manager & Dedicated Sync
  // ==========================================
  listServerConfigs: async (userZomboidDir: string): Promise<any[]> => {
    return await invoke('list_server_configs', { userZomboidDir });
  },

  syncClientToServer: async (filePath: string, activeModIds: string[], activeWorkshopIds: string[]): Promise<void> => {
    return await invoke('sync_client_to_server', { filePath, activeModIds, activeWorkshopIds });
  },

  createNewServerConfig: async (
    userZomboidDir: string,
    serverName: string
  ): Promise<PZServerConfig> => {
    return await invoke('create_new_server_config', { userZomboidDir, serverName });
  },

  deleteServerConfig: async (
    userZomboidDir: string,
    filePath: string,
    serverName?: string
  ): Promise<boolean> => {
    return await invoke('delete_server_config', { userZomboidDir, filePath, serverName });
  },

  launchDedicatedServer: async (
    pzInstallDir: string,
    userZomboidDir: string,
    serverName: string,
    memoryGb?: number,
    nosteam?: boolean,
    adminPassword?: string
  ): Promise<number> => {
    return await invoke('launch_dedicated_server', {
      pzInstallDir,
      userZomboidDir,
      serverName,
      memoryGb,
      nosteam,
      adminPassword,
    });
  },

  stopDedicatedServer: async (userZomboidDir: string, pid?: number): Promise<boolean> => {
    return await invoke('stop_dedicated_server', { userZomboidDir, pid });
  },

  getDedicatedServerStatus: async (userZomboidDir: string): Promise<DedicatedServerStatus> => {
    return await invoke('get_dedicated_server_status', { userZomboidDir });
  },

  getDedicatedServerLogs: async (userZomboidDir: string, maxLines?: number): Promise<string[]> => {
    return await invoke('get_dedicated_server_logs', { userZomboidDir, maxLines });
  },

  saveServerLogSnapshot: async (
    userZomboidDir: string,
    serverName: string,
    customLines?: string[]
  ): Promise<string> => {
    return await invoke('save_server_log_snapshot', {
      userZomboidDir,
      serverName,
      customLines,
    });
  },

  getConnectedPlayers: async (userZomboidDir: string): Promise<ConnectedPlayer[]> => {
    return await invoke('get_connected_players', { userZomboidDir });
  },

  sendServerCommand: async (
    userZomboidDir: string,
    action: string,
    target?: string,
    message?: string,
    reason?: string
  ): Promise<boolean> => {
    return await invoke('send_server_command', {
      userZomboidDir,
      action,
      target,
      message,
      reason,
    });
  },

  getServerQuickSettings: async (filePath: string): Promise<ServerQuickSettings> => {
    return await invoke('get_server_quick_settings', { filePath });
  },

  saveServerQuickSettings: async (filePath: string, settings: ServerQuickSettings): Promise<boolean> => {
    return await invoke('save_server_quick_settings', { filePath, settings });
  },

  // ==========================================
  // Modrinth-Style Instance & Profile Manager
  // ==========================================
  listInstances: async (userZomboidDir: string): Promise<any[]> => {
    return await invoke('list_instances', { userZomboidDir });
  },

  createInstance: async (
    userZomboidDir: string,
    name: string,
    description: string | undefined,
    activeModIds: string[],
    loadOrder: string[]
  ): Promise<any> => {
    return await invoke('create_instance', { userZomboidDir, name, description, activeModIds, loadOrder });
  },

  activateInstance: async (userZomboidDir: string, instanceId: string): Promise<void> => {
    return await invoke('activate_instance', { userZomboidDir, instanceId });
  },

  deleteInstance: async (userZomboidDir: string, instanceId: string): Promise<void> => {
    return await invoke('delete_instance', { userZomboidDir, instanceId });
  },

  updateInstance: async (userZomboidDir: string, instance: any): Promise<any> => {
    return await invoke('update_instance', { userZomboidDir, instance });
  },

  saveMasterLoadOrder: async (
    userZomboidDir: string,
    loadOrder: string[],
    activeModIds: string[]
  ): Promise<void> => {
    return await invoke('save_master_load_order', { userZomboidDir, loadOrder, activeModIds });
  },

  /**
   * Scans the active mod list for cross-mod problems (duplicate ids, dependency
   * cycles, incompatible pairs, shared ModData keys, malformed version directives...).
   *
   * Unlike scanConflicts this NEVER swallows failures: it logs and rethrows so the
   * UI can render a real error state instead of a fake "nothing found" empty state.
   */
  scanModDiagnostics: async (userZomboidDir: string): Promise<ModDiagnostic[]> => {
    try {
      const raw = await invoke<any[]>('scan_mod_diagnostics_cmd', { userZomboidDir });
      if (!raw) return [];
      if (!Array.isArray(raw)) {
        throw new Error('scan_mod_diagnostics_cmd returned a malformed payload (expected an array).');
      }

      const SEVERITIES: DiagnosticSeverity[] = ['Error', 'Warning', 'Info'];

      return raw.map((d) => ({
        kind: String(d.kind ?? 'UNKNOWN'),
        // Rust enum is capitalized; normalize defensively so an unexpected casing never blanks the UI.
        severity: (SEVERITIES.find((s) => s.toLowerCase() === String(d.severity ?? '').toLowerCase()) ?? 'Info'),
        title: String(d.title ?? 'Untitled diagnostic'),
        cause: String(d.cause ?? ''),
        mod_ids: Array.isArray(d.mod_ids) ? d.mod_ids.map(String) : [],
        related_mod_ids: Array.isArray(d.related_mod_ids) ? d.related_mod_ids.map(String) : [],
        file_path: d.file_path ?? null,
        detail: d.detail ?? null,
        suggestion: d.suggestion ?? null,
      }));
    } catch (err) {
      console.error('Mod diagnostics scan failed:', err);
      throw err;
    }
  },

  // ==========================================
  // Safe Fix engine (preview -> confirm -> apply -> rollback)
  // ==========================================

  /**
   * Builds a FixPlan describing every repair the backend proposes. Read-only:
   * nothing is written and no IPC side effect happens outside the scan.
   *
   * Like scanModDiagnostics this NEVER swallows failures — it logs and rethrows
   * so the UI can render a real error state rather than an empty plan.
   */
  previewModFixes: async (userZomboidDir: string): Promise<FixPlan> => {
    try {
      const raw = await invoke<any>('preview_mod_fixes', { userZomboidDir });
      if (!raw || typeof raw !== 'object') {
        throw new Error('preview_mod_fixes returned an empty payload.');
      }
      if (!Array.isArray(raw.changes)) {
        throw new Error('preview_mod_fixes returned a plan without a changes array.');
      }

      return {
        plan_id: String(raw.plan_id ?? ''),
        summary: String(raw.summary ?? ''),
        changes: raw.changes.map((c: any) => ({
          change_id: String(c.change_id ?? ''),
          // Rust enum variants arrive verbatim; pass unknown kinds through.
          kind: String(c.kind ?? 'patch_file') as FixKind,
          target_path: String(c.target_path ?? ''),
          description: String(c.description ?? ''),
          affected_mod_ids: Array.isArray(c.affected_mod_ids) ? c.affected_mod_ids.map(String) : [],
          reversible: c.reversible !== false,
          backup_path: c.backup_path ?? null,
          diff_preview: c.diff_preview ?? null,
        })),
        requires_confirmation: raw.requires_confirmation !== false,
        affected_files: Number(raw.affected_files ?? 0),
      };
    } catch (err) {
      console.error('Fix preview failed:', err);
      throw err;
    }
  },

  /**
   * Applies a previously previewed plan. The backend validates that the plan is
   * still current and rejects it when the mod state moved since the preview —
   * that rejection arrives as a thrown error, which the UI turns into a
   * first-class "stale plan" state rather than a silent failure.
   */
  applyModFix: async (
    userZomboidDir: string,
    planId: string,
    confirmToken: string
  ): Promise<ApplyResult> => {
    try {
      const raw = await invoke<any>('apply_mod_fix', { userZomboidDir, planId, confirmToken });
      if (!raw || typeof raw !== 'object') {
        throw new Error('apply_mod_fix returned an empty payload.');
      }
      if (!Array.isArray(raw.applied)) {
        throw new Error('apply_mod_fix returned a result without an applied list.');
      }

      return {
        applied: raw.applied.map(String),
        skipped: Array.isArray(raw.skipped)
          ? raw.skipped.map((s: any) => ({ change_id: String(s.change_id ?? ''), reason: String(s.reason ?? '') }))
          : [],
        backups: Array.isArray(raw.backups)
          ? raw.backups.map((b: any) => ({
              backup_id: String(b.backup_id ?? ''),
              created_at_unix: Number(b.created_at_unix ?? 0),
              original_path: String(b.original_path ?? ''),
              backup_path: String(b.backup_path ?? ''),
              size_bytes: Number(b.size_bytes ?? 0),
              source: String(b.source ?? ''),
            }))
          : [],
        errors: Array.isArray(raw.errors) ? raw.errors.map(String) : [],
      };
    } catch (err) {
      console.error('Fix apply failed:', err);
      throw err;
    }
  },

  /**
   * Lists rollback points written by previous applies. Read-only.
   */
  listBackups: async (userZomboidDir: string): Promise<BackupEntry[]> => {
    try {
      const raw = await invoke<any[]>('list_backups', { userZomboidDir });
      if (!raw) return [];
      if (!Array.isArray(raw)) {
        throw new Error('list_backups returned a malformed payload (expected an array).');
      }
      return raw.map((b) => ({
        backup_id: String(b.backup_id ?? ''),
        created_at_unix: Number(b.created_at_unix ?? 0),
        original_path: String(b.original_path ?? ''),
        backup_path: String(b.backup_path ?? ''),
        size_bytes: Number(b.size_bytes ?? 0),
        source: String(b.source ?? ''),
      }));
    } catch (err) {
      console.error('Backup list failed:', err);
      throw err;
    }
  },

  /**
   * Restores one backup over its original file. This OVERWRITES the live file,
   * so the UI gates it behind an explicit confirmation.
   */
  restoreBackup: async (userZomboidDir: string, backupId: string): Promise<RestoreResult> => {
    try {
      const raw = await invoke<any>('restore_backup', { userZomboidDir, backupId });
      if (!raw || typeof raw !== 'object') {
        throw new Error('restore_backup returned an empty payload.');
      }
      return {
        restored: Array.isArray(raw.restored) ? raw.restored.map(String) : [],
        errors: Array.isArray(raw.errors) ? raw.errors.map(String) : [],
      };
    } catch (err) {
      console.error('Backup restore failed:', err);
      throw err;
    }
  },
};
