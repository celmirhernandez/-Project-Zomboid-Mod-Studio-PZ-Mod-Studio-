import React, { useState } from 'react';
import { FolderOpen, CheckCircle, AlertTriangle, RefreshCw, HardDrive, Wrench, Plus, Bell, Bot, Copy, Check, Terminal, Radio, Code2, Layers, Info, Lightbulb } from 'lucide-react';
import { PolyfillRule } from '../../types';
import { TauriService } from '../../services/tauri';
import { DiagnosticReportCard } from '../diagnostics/DiagnosticReportCard';

export interface StudioPathsUI {
  pz_install_dir: string;
  workshop_dir: string;
  user_zomboid_dir: string;
  mod_list_ini_path: string;
  is_valid: boolean;
}

interface SettingsModuleProps {
  paths: StudioPathsUI;
  rules: PolyfillRule[];
  onSavePaths: (updatedPaths: StudioPathsUI) => void;
  onToggleRule: (ruleId: string) => void;
  onAutoDetect: () => void;
}

export const SettingsModule: React.FC<SettingsModuleProps> = ({
  paths,
  rules,
  onSavePaths,
  onToggleRule,
  onAutoDetect,
}) => {
  const [activeSubTab, setActiveSubTab] = useState<'PATHS' | 'POLYFILLS' | 'MCP'>('PATHS');
  const [selectedMcpClient, setSelectedMcpClient] = useState<'ANTIGRAVITY' | 'CLAUDE' | 'CURSOR' | 'VSCODE_LOCAL' | 'OPENAI_CODEX'>('ANTIGRAVITY');
  const [copiedMcpConfig, setCopiedMcpConfig] = useState<boolean>(false);
  const [formData, setFormData] = useState<StudioPathsUI>(paths);

  React.useEffect(() => {
    setFormData(paths);
  }, [paths]);

  /** Shows the "importer not wired yet" explainer instead of a raw alert(). */
  const [importerNotice, setImporterNotice] = useState<boolean>(false);
  const [savedSuccess, setSavedSuccess] = useState<boolean>(false);
  const [noticeSilenced, setNoticeSilenced] = useState<boolean>(
    localStorage.getItem('pz_hide_autosort_notice') === 'true'
  );
  const [noMergeWarningSilenced, setNoMergeWarningSilenced] = useState<boolean>(
    localStorage.getItem('pz_skip_no_merge_warning') === 'true'
  );
  const [startupBehavior, setStartupBehavior] = useState<'SHOW_STARTUP_SCREEN' | 'OPEN_LAST_PROFILE'>(() => {
    return (localStorage.getItem('pz_startup_behavior') as any) || 'SHOW_STARTUP_SCREEN';
  });

  const handleSetStartupBehavior = (behavior: 'SHOW_STARTUP_SCREEN' | 'OPEN_LAST_PROFILE') => {
    setStartupBehavior(behavior);
    localStorage.setItem('pz_startup_behavior', behavior);
  };

  const handleRestoreNotice = () => {
    localStorage.removeItem('pz_hide_autosort_notice');
    setNoticeSilenced(false);
  };

  const handleRestoreNoMergeWarning = () => {
    localStorage.removeItem('pz_skip_no_merge_warning');
    setNoMergeWarningSilenced(false);
  };

  const handleSilenceNoMergeWarning = () => {
    localStorage.setItem('pz_skip_no_merge_warning', 'true');
    setNoMergeWarningSilenced(true);
  };

  const handlePickFolder = async (field: keyof StudioPathsUI, currentVal: string) => {
    const selectedFolder = await TauriService.pickFolder(currentVal);
    if (selectedFolder) {
      const updated = {
        ...formData,
        [field]: selectedFolder,
      };
      setFormData(updated);
      onSavePaths(updated);
      setSavedSuccess(true);
      setTimeout(() => setSavedSuccess(false), 3000);
    }
  };

  const getMcpConfigSnippet = () => {
    const binaryPath = 'C:\\Path\\To\\PZ-Mod-Studio-Portable\\pz-mcp-server.exe';
    if (selectedMcpClient === 'CLAUDE') {
      return JSON.stringify(
        {
          mcpServers: {
            'pz-mod-studio': {
              command: binaryPath,
              args: [],
            },
          },
        },
        null,
        2
      );
    } else if (selectedMcpClient === 'CURSOR') {
      return JSON.stringify(
        {
          mcpServers: {
            'pz-mod-studio': {
              command: binaryPath,
              args: [],
            },
          },
        },
        null,
        2
      );
    } else if (selectedMcpClient === 'VSCODE_LOCAL') {
      return JSON.stringify(
        {
          mcpServers: {
            'pz-mod-studio': {
              command: binaryPath,
              args: [],
              disabled: false,
              autoApprove: [],
            },
          },
        },
        null,
        2
      );
    } else if (selectedMcpClient === 'OPENAI_CODEX') {
      return JSON.stringify(
        {
          mcpServers: {
            'pz-mod-studio': {
              command: binaryPath,
              args: [],
              description: 'Project Zomboid Mod Studio MCP Server (Build 41/42)',
            },
          },
        },
        null,
        2
      );
    } else {
      // ANTIGRAVITY / GEMINI CLI
      return JSON.stringify(
        {
          mcpServers: {
            'pz-mod-studio': {
              command: binaryPath,
              args: [],
              env: {},
            },
          },
        },
        null,
        2
      );
    }
  };

  const handleCopyMcpConfig = () => {
    navigator.clipboard.writeText(getMcpConfigSnippet());
    setCopiedMcpConfig(true);
    setTimeout(() => setCopiedMcpConfig(false), 2500);
  };

  return (
    <div className="flex-1 flex flex-col bg-slate-950 text-slate-100 overflow-y-auto p-6">
      <div className="max-w-5xl w-full mx-auto space-y-6">
        {/* Settings Header & Sub-Tab Navigation */}
        <div className="flex items-center justify-between border-b border-slate-800 pb-4">
          <div>
            <h2 className="text-xl font-bold text-slate-100 flex items-center gap-2">
              <HardDrive className="w-5 h-5 text-emerald-400" />
              App Settings & Compatibility Rules
            </h2>
            <p className="text-xs text-slate-400 mt-1">
              Configure local game paths and enable/disable B42+ runtime polyfill rules.
            </p>
          </div>

          {/* Sub-Tab Selector Buttons */}
          <div className="bg-slate-900 border border-slate-800 rounded-lg p-1 flex gap-1 text-xs">
            <button
              onClick={() => setActiveSubTab('PATHS')}
              className={`px-3 py-1.5 rounded-md font-medium transition cursor-pointer flex items-center gap-1.5 ${
                activeSubTab === 'PATHS'
                  ? 'bg-slate-800 text-emerald-400 font-bold border border-slate-700 shadow-sm'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <HardDrive className="w-3.5 h-3.5" />
              Directory Paths
            </button>
            <button
              onClick={() => setActiveSubTab('POLYFILLS')}
              className={`px-3 py-1.5 rounded-md font-medium transition cursor-pointer flex items-center gap-1.5 ${
                activeSubTab === 'POLYFILLS'
                  ? 'bg-slate-800 text-emerald-400 font-bold border border-slate-700 shadow-sm'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Wrench className="w-3.5 h-3.5" />
              Polyfill Rules ({rules.filter((r) => r.enabled).length})
            </button>
            <button
              onClick={() => setActiveSubTab('MCP')}
              className={`px-3 py-1.5 rounded-md font-medium transition cursor-pointer flex items-center gap-1.5 ${
                activeSubTab === 'MCP'
                  ? 'bg-slate-800 text-cyan-400 font-bold border border-slate-700 shadow-sm'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Bot className="w-3.5 h-3.5 text-cyan-400" />
              MCP Agent Server
            </button>
          </div>
        </div>

        {savedSuccess && (
          <div className="p-3 bg-emerald-950/80 border border-emerald-500/40 rounded-lg text-xs text-emerald-300 flex items-center gap-2">
            <CheckCircle className="w-4 h-4 text-emerald-400" />
            Settings saved and validated successfully!
          </div>
        )}

        {/* SUB-TAB 1: Directory Paths */}
        {activeSubTab === 'PATHS' && (
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-400 uppercase tracking-wider">
                System Directory Paths
              </span>

              <div className="flex items-center gap-2">
                <button
                  onClick={onAutoDetect}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg border border-slate-700 transition cursor-pointer"
                >
                  <RefreshCw className="w-3.5 h-3.5 text-cyan-400" />
                  Auto-Detect Paths
                </button>
              </div>
            </div>

            {/* Card 1: PZ Installation Path */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-xs font-bold text-slate-200">
                  Project Zomboid Installation Directory
                </label>
                {formData.pz_install_dir ? (
                  <span className="text-[10px] text-emerald-400 flex items-center gap-1">
                    <CheckCircle className="w-3 h-3" /> Valid Path
                  </span>
                ) : (
                  <span className="text-[10px] text-amber-400 flex items-center gap-1">
                    <AlertTriangle className="w-3 h-3" /> Path Not Found
                  </span>
                )}
              </div>
              <p className="text-[11px] text-slate-400">
                Root installation folder containing <span className="font-mono text-slate-300">ProjectZomboid64.exe</span> and base game files. Compatible with Steam, GOG or manual installations.
              </p>
              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={formData.pz_install_dir}
                  className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs font-mono text-slate-300 cursor-not-allowed"
                />
                <button
                  onClick={() => handlePickFolder('pz_install_dir', formData.pz_install_dir)}
                  className="flex items-center gap-1.5 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg border border-slate-700 transition cursor-pointer"
                >
                  <FolderOpen className="w-4 h-4 text-emerald-400" />
                  Browse...
                </button>
              </div>
            </div>

            {/* Card 2: Steam Workshop / External Mods Path */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-xs font-bold text-slate-200">
                  Steam Workshop Folder (Optional if not using Steam)
                </label>
                {formData.workshop_dir ? (
                  <span className="text-[10px] text-slate-400 font-mono">Steam App ID: 108600</span>
                ) : (
                  <span className="text-[10px] text-cyan-300 font-mono bg-cyan-950/60 px-2 py-0.5 rounded border border-cyan-800/50">
                    Optional (GOG / No-Steam)
                  </span>
                )}
              </div>
              <p className="text-[11px] text-slate-400">
                For Steam users: Path where Workshop mods are downloaded (<span className="font-mono text-slate-300">workshop/content/108600</span>). If you play on GOG or install mods manually in <span className="font-mono text-slate-300">Zomboid/mods</span>, you can leave this field empty.
              </p>
              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={formData.workshop_dir}
                  placeholder="(Optional for GOG or No-Steam versions)"
                  className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs font-mono text-slate-300 cursor-not-allowed"
                />
                <button
                  onClick={() => handlePickFolder('workshop_dir', formData.workshop_dir)}
                  className="flex items-center gap-1.5 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg border border-slate-700 transition cursor-pointer"
                >
                  <FolderOpen className="w-4 h-4 text-emerald-400" />
                  Browse...
                </button>
              </div>
            </div>

            {/* Card 3: User Zomboid Directory */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-xs font-bold text-slate-200">
                  User Zomboid Directory (Data and Local Mods Folder)
                </label>
                {formData.user_zomboid_dir ? (
                  <span className="text-[10px] text-emerald-400 flex items-center gap-1">
                    <CheckCircle className="w-3 h-3" /> Universal
                  </span>
                ) : (
                  <span className="text-[10px] text-amber-400 flex items-center gap-1">
                    <AlertTriangle className="w-3 h-3" /> Required
                  </span>
                )}
              </div>
              <p className="text-[11px] text-slate-400">
                User personal folder (<span className="font-mono text-slate-300">C:\Users\&lt;User&gt;\Zomboid</span>). Contains logs (<span className="font-mono text-slate-300">console.txt</span>), saves, profiles and the local mods folder (<span className="font-mono text-slate-300">Zomboid/mods</span>).
              </p>
              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={formData.user_zomboid_dir}
                  className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs font-mono text-slate-300 cursor-not-allowed"
                />
                <button
                  onClick={() => handlePickFolder('user_zomboid_dir', formData.user_zomboid_dir)}
                  className="flex items-center gap-1.5 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg border border-slate-700 transition cursor-pointer"
                >
                  <FolderOpen className="w-4 h-4 text-emerald-400" />
                  Browse...
                </button>
              </div>
            </div>

            {/* Card 4: ModListData.ini Path */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-2">
              <label className="text-xs font-bold text-slate-200">
                Mod Configuration File (ModListData.ini / default.txt)
              </label>
              <p className="text-[11px] text-slate-400">
                File that stores active mods and load order for Project Zomboid. Automatically detected in <span className="font-mono text-slate-300">Zomboid/mods/ModListData.ini</span> or <span className="font-mono text-slate-300">Zomboid/default.txt</span>.
              </p>
              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={formData.mod_list_ini_path}
                  className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs font-mono text-slate-300 cursor-not-allowed"
                />
                <button
                  onClick={() => handlePickFolder('mod_list_ini_path', formData.mod_list_ini_path)}
                  className="flex items-center gap-1.5 px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg border border-slate-700 transition cursor-pointer"
                >
                  <FolderOpen className="w-4 h-4 text-emerald-400" />
                  Browse...
                </button>
              </div>
            </div>

            {/* Card 5: Unified App Notification & Warning Preferences */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-4">
              <div className="flex items-center justify-between border-b border-slate-800/80 pb-3">
                <label className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
                  <Bell className="w-4 h-4 text-emerald-400" />
                  System Notifications and Warnings
                </label>
                <span className="text-[10px] font-bold font-mono px-2 py-0.5 rounded bg-slate-950 text-slate-400 border border-slate-800">
                  UI Preferences
                </span>
              </div>
              <p className="text-[11px] text-slate-400">
                Configure the display of warnings, security confirmations and application popups.
              </p>

              <div className="space-y-3">
                {/* 1. Advertencia de Lanzamiento sin Merge */}
                <div className="p-3.5 bg-slate-950/60 border border-slate-800 rounded-xl flex flex-col md:flex-row md:items-center justify-between gap-3">
                  <div className="space-y-1 max-w-xl">
                    <div className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
                      <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                      <span>Warning when launching game without active merge package</span>
                    </div>
                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      Shows a preventive warning before running the game if you have no active merge patch to resolve collisions between mods.
                    </p>
                  </div>

                  <div className="shrink-0 flex items-center gap-2">
                    {noMergeWarningSilenced ? (
                      <button
                        onClick={handleRestoreNoMergeWarning}
                        className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg text-xs font-bold transition cursor-pointer shadow"
                      >
                        Restore Warning
                      </button>
                    ) : (
                      <button
                        onClick={handleSilenceNoMergeWarning}
                        className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-medium border border-slate-700 transition cursor-pointer"
                      >
                        Silence Warning
                      </button>
                    )}
                  </div>
                </div>

                {/* 2. Notificación de Auto-Sort */}
                <div className="p-3.5 bg-slate-950/60 border border-slate-800 rounded-xl flex flex-col md:flex-row md:items-center justify-between gap-3">
                  <div className="space-y-1 max-w-xl">
                    <div className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
                      <RefreshCw className="w-3.5 h-3.5 text-cyan-400" />
                      <span>Auto-Sort Dependencies Explanatory Message</span>
                    </div>
                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      Shows an explanatory informational dialog when clicking the sort dependencies button in the mod list.
                    </p>
                  </div>

                  <div className="shrink-0 flex items-center gap-2">
                    {noticeSilenced ? (
                      <button
                        onClick={handleRestoreNotice}
                        className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg text-xs font-bold transition cursor-pointer shadow"
                      >
                        Restore Notification
                      </button>
                    ) : (
                      <button
                        onClick={() => {
                          localStorage.setItem('pz_hide_autosort_notice', 'true');
                          setNoticeSilenced(true);
                        }}
                        className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs font-medium border border-slate-700 transition cursor-pointer"
                      >
                        Silence Notification
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>

            {/* Card 5b: Diagnostic Report (bug report bundle) */}
            <DiagnosticReportCard userZomboidDir={paths.user_zomboid_dir} />

            {/* Card 6: Startup Screen Behavior */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-3">
              <div className="flex items-center justify-between">
                <label className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
                  <Layers className="w-4 h-4 text-emerald-400" />
                  Startup Behavior
                </label>
                <span className="text-[10px] font-bold font-mono px-2 py-0.5 rounded bg-slate-950 text-emerald-300 border border-slate-800">
                  {startupBehavior === 'SHOW_STARTUP_SCREEN' ? 'Profiles Screen' : 'Last Active Profile'}
                </span>
              </div>
              <p className="text-[11px] text-slate-400">
                Select which module you want to open automatically when starting PZ Mod Studio:
              </p>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
                <div
                  onClick={() => handleSetStartupBehavior('SHOW_STARTUP_SCREEN')}
                  className={`p-3.5 rounded-xl border transition cursor-pointer flex items-start gap-3 ${
                    startupBehavior === 'SHOW_STARTUP_SCREEN'
                      ? 'bg-emerald-950/40 border-emerald-500 text-slate-100 ring-1 ring-emerald-500/50'
                      : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                  }`}
                >
                  <input
                    type="radio"
                    name="startupBehavior"
                    checked={startupBehavior === 'SHOW_STARTUP_SCREEN'}
                    onChange={() => handleSetStartupBehavior('SHOW_STARTUP_SCREEN')}
                    className="mt-1 text-emerald-500 focus:ring-0 cursor-pointer"
                  />
                  <div className="space-y-1 text-left">
                    <div className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
                      <span>Profiles Screen (Start)</span>
                      <span className="text-[10px] bg-emerald-950 text-emerald-300 border border-emerald-700/60 px-1.5 py-0.2 rounded font-mono">Recommended</span>
                    </div>
                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      Shows the profile manager so you can choose or activate the combination you are going to work with.
                    </p>
                  </div>
                </div>

                <div
                  onClick={() => handleSetStartupBehavior('OPEN_LAST_PROFILE')}
                  className={`p-3.5 rounded-xl border transition cursor-pointer flex items-start gap-3 ${
                    startupBehavior === 'OPEN_LAST_PROFILE'
                      ? 'bg-emerald-950/40 border-emerald-500 text-slate-100 ring-1 ring-emerald-500/50'
                      : 'bg-slate-950/60 border-slate-800 text-slate-400 hover:border-slate-700 hover:text-slate-200'
                  }`}
                >
                  <input
                    type="radio"
                    name="startupBehavior"
                    checked={startupBehavior === 'OPEN_LAST_PROFILE'}
                    onChange={() => handleSetStartupBehavior('OPEN_LAST_PROFILE')}
                    className="mt-1 text-emerald-500 focus:ring-0 cursor-pointer"
                  />
                  <div className="space-y-1 text-left">
                    <div className="text-xs font-bold text-slate-200">
                      Open Last Profile Directly
                    </div>
                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      Loads the previous active profile and takes you directly to the mod list.
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* SUB-TAB 2: Polyfill & Compatibility Rules Matrix */}
        {activeSubTab === 'POLYFILLS' && (
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-400 uppercase tracking-wider">
                Build 42+ Polyfill Rules Matrix
              </span>

              <button
                onClick={() => setImporterNotice(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-bold bg-emerald-950 hover:bg-emerald-900 text-emerald-300 border border-emerald-800 rounded-lg shadow transition cursor-pointer"
              >
                <Plus className="w-4 h-4" />
                Import Community Rule (.json)
              </button>
            </div>

            {/* Placeholder notice for the not-yet-wired community rule importer. */}
            {importerNotice && (
            <div className="bg-cyan-950/40 border border-cyan-800/60 border-l-4 border-l-cyan-500 rounded-lg px-3.5 py-2.5 flex items-start gap-2 animate-fade-in">
              <Info className="w-4 h-4 text-cyan-400 shrink-0 mt-px" />
              <div className="min-w-0 flex-1">
                <div className="text-[11px] font-bold uppercase tracking-wider text-cyan-300">
                  Community Rule Importer
                </div>
                <div className="text-[11px] text-slate-200 leading-relaxed mt-0.5">
                  Select a <span className="font-mono text-cyan-300">poly_rules.json</span> file to import extra
                  rules. This importer is not wired to the backend yet — no file dialog opens and nothing is
                  written. The built-in rules below are the only active source.
                </div>
              </div>
              <button
                onClick={() => setImporterNotice(false)}
                className="text-slate-500 hover:text-slate-200 transition cursor-pointer text-[11px] shrink-0"
                title="Dismiss"
              >
                Dismiss
              </button>
            </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {rules.map((rule) => (
                <div
                  key={rule.id}
                  className={`bg-slate-900/80 border rounded-xl p-4 flex flex-col justify-between space-y-3 transition ${
                    rule.enabled
                      ? 'border-emerald-500/40 shadow-sm'
                      : 'border-slate-800 opacity-60'
                  }`}
                >
                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-[10px] px-2 py-0.5 rounded bg-slate-950 text-slate-400 border border-slate-800 font-semibold">
                        {rule.category}
                      </span>
                      <button
                        onClick={() => onToggleRule(rule.id)}
                        className={`w-10 h-5 rounded-full p-0.5 transition cursor-pointer ${
                          rule.enabled ? 'bg-emerald-500 justify-end' : 'bg-slate-800 justify-start'
                        } flex items-center`}
                      >
                        <div className="w-4 h-4 rounded-full bg-white shadow-md"></div>
                      </button>
                    </div>

                    <h4 className="text-xs font-bold text-slate-100 flex items-center gap-1.5">
                      <Wrench className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                      {rule.name}
                    </h4>

                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      {rule.description}
                    </p>
                  </div>

                  <div className="pt-2 border-t border-slate-800/60 flex items-center justify-between text-[10px] font-mono text-slate-500">
                    <span>Rule ID: {rule.id}</span>
                    <span className={rule.enabled ? 'text-emerald-400 font-bold' : 'text-slate-600'}>
                      {rule.enabled ? 'Active' : 'Disabled'}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* SUB-TAB 3: Model Context Protocol (MCP) Agent Server */}
        {activeSubTab === 'MCP' && (
          <div className="space-y-6">
            {/* Header Banner */}
            <div className="bg-gradient-to-r from-slate-900 via-slate-900/90 to-cyan-950/40 border border-cyan-500/30 rounded-xl p-5 shadow-lg">
              <div className="flex items-start justify-between">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="p-1.5 rounded-lg bg-cyan-950 text-cyan-400 border border-cyan-800">
                      <Bot className="w-5 h-5" />
                    </span>
                    <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                      Model Context Protocol (MCP) Server
                      <span className="text-[10px] font-mono font-semibold px-2 py-0.5 bg-emerald-950 text-emerald-300 border border-emerald-800 rounded-full flex items-center gap-1">
                        <Radio className="w-2.5 h-2.5 text-emerald-400 animate-pulse" />
                        Stdio Ready
                      </span>
                    </h3>
                  </div>
                  <p className="text-xs text-slate-300 max-w-3xl leading-relaxed">
                    Connect autonomous AI agents (Antigravity, Claude Desktop, Cursor, VS Code with Roo Code/Cline, or local models via Ollama/LM Studio) directly to PZ Mod Studio. The MCP server provides real-time access to console logs, crash diagnostics, VFS collision detection, Lua AST validation and live game control.
                  </p>
                </div>
              </div>
            </div>

            {/* Quick Setup / Client Configuration */}
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-5 space-y-4">
              <div className="flex flex-col md:flex-row md:items-center justify-between gap-3">
                <div>
                  <h4 className="text-xs font-bold text-slate-200 uppercase tracking-wider flex items-center gap-1.5">
                    <Terminal className="w-4 h-4 text-cyan-400" />
                    Configuration Generator for MCP Clients
                  </h4>
                  <p className="text-[11px] text-slate-400 mt-0.5">
                    Select your AI client or environment and copy the JSON snippet into your configuration file:
                  </p>
                </div>

                {/* Client Selector Pills */}
                <div className="bg-slate-950 border border-slate-800 rounded-lg p-1 flex flex-wrap gap-1 text-[11px]">
                  <button
                    onClick={() => setSelectedMcpClient('ANTIGRAVITY')}
                    className={`px-2.5 py-1 rounded font-medium transition cursor-pointer ${
                      selectedMcpClient === 'ANTIGRAVITY'
                        ? 'bg-cyan-950 text-cyan-300 font-bold border border-cyan-800'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    Antigravity / Gemini
                  </button>
                  <button
                    onClick={() => setSelectedMcpClient('CLAUDE')}
                    className={`px-2.5 py-1 rounded font-medium transition cursor-pointer ${
                      selectedMcpClient === 'CLAUDE'
                        ? 'bg-cyan-950 text-cyan-300 font-bold border border-cyan-800'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    Claude Desktop
                  </button>
                  <button
                    onClick={() => setSelectedMcpClient('CURSOR')}
                    className={`px-2.5 py-1 rounded font-medium transition cursor-pointer ${
                      selectedMcpClient === 'CURSOR'
                        ? 'bg-cyan-950 text-cyan-300 font-bold border border-cyan-800'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    Cursor / Windsurf
                  </button>
                  <button
                    onClick={() => setSelectedMcpClient('VSCODE_LOCAL')}
                    className={`px-2.5 py-1 rounded font-medium transition cursor-pointer ${
                      selectedMcpClient === 'VSCODE_LOCAL'
                        ? 'bg-cyan-950 text-cyan-300 font-bold border border-cyan-800'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    VS Code (Roo / Cline / Ollama Local)
                  </button>
                  <button
                    onClick={() => setSelectedMcpClient('OPENAI_CODEX')}
                    className={`px-2.5 py-1 rounded font-medium transition cursor-pointer ${
                      selectedMcpClient === 'OPENAI_CODEX'
                        ? 'bg-cyan-950 text-cyan-300 font-bold border border-cyan-800'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    OpenAI / Codex / ChatGPT
                  </button>
                </div>
              </div>

              {/* JSON Code Box */}
              <div className="relative bg-slate-950 border border-slate-800 rounded-lg p-4 font-mono text-xs text-slate-300 overflow-x-auto">
                <button
                  onClick={handleCopyMcpConfig}
                  className={`absolute top-3 right-3 px-3 py-1.5 rounded-md text-xs font-semibold flex items-center gap-1.5 transition cursor-pointer shadow ${
                    copiedMcpConfig
                      ? 'bg-emerald-600 text-white'
                      : 'bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700'
                  }`}
                >
                  {copiedMcpConfig ? (
                    <>
                      <Check className="w-3.5 h-3.5" />
                      Copied!
                    </>
                  ) : (
                    <>
                      <Copy className="w-3.5 h-3.5" />
                      Copy MCP JSON
                    </>
                  )}
                </button>
                <pre>{getMcpConfigSnippet()}</pre>
              </div>

              {/* Step-by-Step Instructions per Client */}
              {selectedMcpClient === 'CLAUDE' && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-4 space-y-2.5 text-xs">
                  <div className="flex items-center gap-2 text-cyan-300 font-bold">
                    <Info className="w-4 h-4 text-cyan-400" />
                    <span>How to setup in Claude Desktop:</span>
                  </div>
                  <ol className="list-decimal list-inside space-y-1.5 text-slate-300 leading-relaxed pl-1 text-[11px]">
                    <li>Open <b>Claude Desktop</b> and go to <b>File &gt; Settings</b> (or press <kbd className="px-1 py-0.5 rounded bg-slate-800 font-mono text-[10px] text-slate-200">Ctrl + ,</kbd>).</li>
                    <li>Select the <b>Developer</b> tab and click <b>Edit Config</b> to open <code className="bg-slate-900 px-1.5 py-0.5 rounded text-cyan-300 font-mono text-[10px]">claude_desktop_config.json</code>.</li>
                    <li>Paste the JSON configuration above inside the file and save it.</li>
                    <li>Restart Claude Desktop. In new chats, you will see a <b>🔨 Hammer</b> icon with 22 PZ Mod Studio tools!</li>
                  </ol>
                  <div className="p-2 rounded bg-cyan-950/30 border border-cyan-900/60 text-[11px] text-cyan-200">
                    💬 <b>Example Prompt:</b> <i>"Read console.txt to diagnose why Project Zomboid crashed"</i> or <i>"Sort my mod load order"</i>.
                  </div>
                </div>
              )}

              {selectedMcpClient === 'CURSOR' && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-4 space-y-2.5 text-xs">
                  <div className="flex items-center gap-2 text-cyan-300 font-bold">
                    <Info className="w-4 h-4 text-cyan-400" />
                    <span>How to setup in Cursor / Windsurf IDE:</span>
                  </div>
                  <ol className="list-decimal list-inside space-y-1.5 text-slate-300 leading-relaxed pl-1 text-[11px]">
                    <li>Open <b>Cursor Settings</b> (Gear icon) &gt; <b>Features</b> &gt; <b>MCP</b>.</li>
                    <li>Click <b>+ Add New MCP Server</b>.</li>
                    <li>Set <b>Type</b>: <code className="bg-slate-900 px-1 py-0.5 rounded text-cyan-300 font-mono">command</code>, <b>Name</b>: <code className="bg-slate-900 px-1 py-0.5 rounded text-cyan-300 font-mono">pz-mod-studio</code>, and <b>Command</b>: <code className="bg-slate-900 px-1 py-0.5 rounded text-cyan-300 font-mono">C:\Path\To\PZ-Mod-Studio-Portable\pz-mcp-server.exe</code>.</li>
                    <li>The status dot will turn green (Enabled), exposing Lua AST and live diagnostics to the Agent.</li>
                  </ol>
                  <div className="p-2 rounded bg-cyan-950/30 border border-cyan-900/60 text-[11px] text-cyan-200">
                    💬 <b>Example Prompt:</b> <i>"@pz-mod-studio scan active mod collisions in VFS and merge conflicting scripts"</i>.
                  </div>
                </div>
              )}

              {selectedMcpClient === 'VSCODE_LOCAL' && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-4 space-y-2.5 text-xs">
                  <div className="flex items-center gap-2 text-cyan-300 font-bold">
                    <Info className="w-4 h-4 text-cyan-400" />
                    <span>How to setup in VS Code (Roo Code / Cline with Local or Cloud LLMs):</span>
                  </div>
                  <ol className="list-decimal list-inside space-y-1.5 text-slate-300 leading-relaxed pl-1 text-[11px]">
                    <li>Install the <b>Roo Code</b> or <b>Cline</b> extension from the VS Code Marketplace.</li>
                    <li>Click the extension icon on the sidebar, click the <b>MCP Servers (Plugs)</b> tab at the top and choose <b>Configure MCP</b>.</li>
                    <li>Paste the JSON snippet above into <code className="bg-slate-900 px-1.5 py-0.5 rounded text-cyan-300 font-mono text-[10px]">cline_mcp_settings.json</code> / <code className="bg-slate-900 px-1.5 py-0.5 rounded text-cyan-300 font-mono text-[10px]">roo_mcp_settings.json</code>.</li>
                    <li>You can use Cloud models (Claude 3.7 / GPT-4o) or 100% Offline with <b>Ollama / LM Studio</b> (e.g. <i>Qwen 2.5 Coder</i>).</li>
                  </ol>
                  <div className="p-2 rounded bg-emerald-950/30 border border-emerald-900/60 text-[11px] text-emerald-200">
                    🔒 <b>100% Offline Modding:</b> Local models can query PZ Mod Studio VFS collision tools and parse Lua without needing any internet connection.
                  </div>
                </div>
              )}

              {selectedMcpClient === 'ANTIGRAVITY' && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-4 space-y-2.5 text-xs">
                  <div className="flex items-center gap-2 text-cyan-300 font-bold">
                    <Info className="w-4 h-4 text-cyan-400" />
                    <span>How to setup in Antigravity / Gemini CLI:</span>
                  </div>
                  <ol className="list-decimal list-inside space-y-1.5 text-slate-300 leading-relaxed pl-1 text-[11px]">
                    <li>Open your Antigravity or Gemini CLI configuration file (<code className="bg-slate-900 px-1.5 py-0.5 rounded text-cyan-300 font-mono text-[10px]">.gemini/settings.json</code> or user settings).</li>
                    <li>Paste the configuration snippet under the <code className="bg-slate-900 px-1.5 py-0.5 rounded text-cyan-300 font-mono text-[10px]">"mcpServers"</code> section.</li>
                    <li>Antigravity automatically discovers and connects all 22 tools and 8 resources without any manual intervention.</li>
                  </ol>
                  <div className="p-2 rounded bg-cyan-950/30 border border-cyan-900/60 text-[11px] text-cyan-200">
                    💬 <b>Example Prompt:</b> <i>"Inspect console.txt and generate a master polyfill patch for missing tables"</i>.
                  </div>
                </div>
              )}

              {selectedMcpClient === 'OPENAI_CODEX' && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-4 space-y-2.5 text-xs">
                  <div className="flex items-center gap-2 text-cyan-300 font-bold">
                    <Info className="w-4 h-4 text-cyan-400" />
                    <span>How to setup in OpenAI / Codex / Custom GPT MCP Bridges:</span>
                  </div>
                  <ol className="list-decimal list-inside space-y-1.5 text-slate-300 leading-relaxed pl-1 text-[11px]">
                    <li>Configure your MCP-compatible client or local stdio bridge with the command executable.</li>
                    <li>The server will expose tool definitions compliant with the <b>JSON-RPC 2.0 MCP standard (2024-11-05)</b>.</li>
                    <li>Ensure the process is spawned with standard I/O pipes (stdio).</li>
                  </ol>
                </div>
              )}

              {/* FAQ / How Stdio Works Card */}
              <div className="p-3.5 bg-amber-950/20 border border-amber-500/30 rounded-lg text-xs text-amber-200 space-y-1.5">
                <div className="flex items-center gap-1.5 font-bold text-amber-300">
                  <Lightbulb className="w-4 h-4 text-amber-400 shrink-0" />
                  <span>Important: Do NOT run pz-mcp-server.exe directly by double-clicking</span>
                </div>
                <p className="text-[11px] text-amber-300/80 leading-relaxed">
                  If you double-click <code className="bg-slate-950 px-1 py-0.5 rounded font-mono text-[10px] text-amber-200">pz-mcp-server.exe</code>, a black command prompt will open and stay blank. <b>This is completely normal.</b> The MCP server communicates silently over <code className="font-mono text-amber-100 font-semibold">stdio</code> (JSON-RPC) with AI agents. You do not need to keep it open; your AI assistant (Claude, Cursor, Antigravity) will launch and close it automatically in the background whenever you send prompts in chat.
                </p>
              </div>

              <div className="text-[11px] text-slate-400 flex flex-wrap items-center gap-2 pt-1 border-t border-slate-800/60">
                <span className="font-semibold text-slate-300">Executable:</span>
                <code className="bg-slate-950 px-2 py-0.5 rounded border border-slate-800 text-cyan-300 font-mono text-[10px]">
                  &lt;PZ-Mod-Studio-Folder&gt;\PZ-Mod-Studio-Portable\pz-mcp-server.exe
                </code>
                <span>or CLI flag:</span>
                <code className="bg-slate-950 px-2 py-0.5 rounded border border-slate-800 text-cyan-300 font-mono text-[10px]">
                  Project-Zomboid-Mod-Studio.exe --mcp
                </code>
              </div>
            </div>

            {/* MCP Tools (22 Tools) & Resources (4 URIs) Overview */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
              {/* Exposed Tools (Span 2 cols) */}
              <div className="lg:col-span-2 bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-4">
                <div className="flex items-center justify-between pb-2 border-b border-slate-800">
                  <h4 className="text-xs font-bold text-slate-200 uppercase tracking-wider flex items-center gap-1.5">
                    <Code2 className="w-4 h-4 text-emerald-400" />
                    MCP Tools Catalog (22 Tools Available)
                  </h4>
                  <span className="text-[10px] font-mono text-emerald-400 font-bold bg-emerald-950/60 px-2 py-0.5 rounded border border-emerald-800">
                    tools/call
                  </span>
                </div>

                <div className="space-y-3.5 max-h-[520px] overflow-y-auto pr-1">
                  {/* Category 1: Game Control & IPC Bridge */}
                  <div className="space-y-1.5">
                    <span className="text-[10px] font-bold text-cyan-400 uppercase tracking-wider block">
                      🎮 Process Control & Live IPC Bridge (6 Tools)
                    </span>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">get_game_status</div>
                        <div className="text-[10px] text-slate-400">Checks if ProjectZomboid64.exe is running and returns its PID.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">launch_game</div>
                        <div className="text-[10px] text-slate-400">Launches the game with configurable debugging flags (-debug, -windowed).</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">terminate_game</div>
                        <div className="text-[10px] text-slate-400">Closes or forces the termination of the Project Zomboid process.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">send_game_ipc_command</div>
                        <div className="text-[10px] text-slate-400">Sends live commands (give_item, eval_lua, godmode) to the companion mod.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">get_game_ipc_response</div>
                        <div className="text-[10px] text-slate-400">Reads the execution response emitted by the companion mod in the game.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-cyan-300 font-bold text-[11px]">install_bridge_companion_mod</div>
                        <div className="text-[10px] text-slate-400">Installs the companion mod Z_PZModStudio_Bridge in Zomboid/mods.</div>
                      </div>
                    </div>
                  </div>

                  {/* Category 2: Monitor Center & Diagnostics */}
                  <div className="space-y-1.5 pt-1">
                    <span className="text-[10px] font-bold text-amber-400 uppercase tracking-wider block">
                      🩺 Monitor Center & Crash Diagnostics (4 Tools)
                    </span>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-amber-300 font-bold text-[11px]">get_monitor_logs</div>
                        <div className="text-[10px] text-slate-400">Reads and filters recent lines from console.txt (errors and stacktraces).</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-amber-300 font-bold text-[11px]">get_crash_diagnostics</div>
                        <div className="text-[10px] text-slate-400">Parses Java/Lua exceptions and generates B41/B42 diagnostics with solutions.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-amber-300 font-bold text-[11px]">list_available_logs</div>
                        <div className="text-[10px] text-slate-400">Lists all session log files on disk (console.txt and Logs/).</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-amber-300 font-bold text-[11px]">read_log_file</div>
                        <div className="text-[10px] text-slate-400">Reads any specific log file with exception filtering.</div>
                      </div>
                    </div>
                  {/* Category 3: Mods, Paths & Profiles */}
                  <div className="space-y-1.5 pt-1">
                    <span className="text-[10px] font-bold text-emerald-400 uppercase tracking-wider block">
                      📦 Mods, Paths & Profiles (7 Tools)
                    </span>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">get_studio_paths</div>
                        <div className="text-[10px] text-slate-400">Auto-detects game paths, Workshop and Zomboid folder.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">list_installed_mods</div>
                        <div className="text-[10px] text-slate-400">Scans and lists all Steam Workshop and local mods.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">sort_mod_load_order</div>
                        <div className="text-[10px] text-slate-400">Topological sorting of dependencies and cycle detection.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">scan_mod_conflicts</div>
                        <div className="text-[10px] text-slate-400">Scans VFS collisions between mods and the base game.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">list_mod_profiles</div>
                        <div className="text-[10px] text-slate-400">Lists all saved, active mod profiles and order.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">create_mod_profile</div>
                        <div className="text-[10px] text-slate-400">Creates a new independent mod combination profile.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-emerald-300 font-bold text-[11px]">activate_mod_profile</div>
                        <div className="text-[10px] text-slate-400">Activates a mod profile in default.txt / ModListData.ini.</div>
                      </div>
                    </div>
                  </div>

                  {/* Category 4: AST Merger & Patch Engine */}
                  <div className="space-y-1.5 pt-1">
                    <span className="text-[10px] font-bold text-purple-400 uppercase tracking-wider block">
                      ⚡ AST Merge Engine & Patches (5 Tools)
                    </span>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-purple-300 font-bold text-[11px]">validate_lua_syntax</div>
                        <div className="text-[10px] text-slate-400">Validates Lua syntax using AST parser (full_moon) with line and column.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-purple-300 font-bold text-[11px]">merge_lua_scripts</div>
                        <div className="text-[10px] text-slate-400">3-way AST merge between base code and two conflicting variants.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-purple-300 font-bold text-[11px]">list_merged_packages</div>
                        <div className="text-[10px] text-slate-400">Lists all patch and merge packages (Z_PZModStudio_*).</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-purple-300 font-bold text-[11px]">get_master_patch_status</div>
                        <div className="text-[10px] text-slate-400">Queries the Master Patch status and saved resolutions.</div>
                      </div>
                      <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                        <div className="font-mono text-purple-300 font-bold text-[11px]">save_draft_resolution</div>
                        <div className="text-[10px] text-slate-400">Saves a code resolution directly into the patch package.</div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              {/* Exposed Resources (1 col) */}
              <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-3">
                <div className="flex items-center justify-between pb-2 border-b border-slate-800">
                  <h4 className="text-xs font-bold text-slate-200 uppercase tracking-wider flex items-center gap-1.5">
                    <Layers className="w-4 h-4 text-purple-400" />
                    MCP Resources (8 Passive URIs)
                  </h4>
                  <span className="text-[10px] font-mono text-purple-300 font-bold bg-purple-950/60 px-2 py-0.5 rounded border border-purple-800">
                    resources/read
                  </span>
                </div>

                <div className="space-y-2 max-h-[520px] overflow-y-auto pr-1 text-xs">
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://monitor/console-log</div>
                    <div className="text-[10px] text-slate-400">Live text stream of the console.txt file.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://game/status</div>
                    <div className="text-[10px] text-slate-400">Game execution status, PID and companion bridge.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://diagnostics/latest-crash</div>
                    <div className="text-[10px] text-slate-400">Parsed diagnostic of the last crash and stacktraces.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://mods/installed-summary</div>
                    <div className="text-[10px] text-slate-400">JSON snapshot of all installed mods and order.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://profiles/list</div>
                    <div className="text-[10px] text-slate-400">Complete list of saved and active mod profiles.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://conflicts/active</div>
                    <div className="text-[10px] text-slate-400">Snapshot of VFS collisions between active mods.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://patches/status</div>
                    <div className="text-[10px] text-slate-400">Status and resolutions of Z_PZModStudio_MasterPatch.</div>
                  </div>
                  <div className="p-2 rounded-lg bg-slate-950 border border-slate-800 space-y-0.5">
                    <div className="font-mono text-purple-300 font-bold text-[11px]">pz://paths/config</div>
                    <div className="text-[10px] text-slate-400">Detected system paths and configuration.</div>
                  </div>
                </div>
              </div>
            </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
