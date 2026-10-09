import React, { useState, useEffect, useCallback } from 'react';
import { ActiveTab, VfsConflict, PolyfillRule, ModInfo, TranslatedErrorCard, GameLaunchMode, AppProfile } from './types';
import { DEFAULT_POLYFILL_RULES } from './data/default_rules';
import { StudioHeader } from './components/layout/StudioHeader';
import { StudioSidebar } from './components/layout/StudioSidebar';
import { MergerModule } from './components/merger/MergerModule';
import { LoadOrderModule } from './components/load_order/LoadOrderModule';
import { SandboxModule } from './components/sandbox/SandboxModule';
import { SettingsModule, StudioPathsUI } from './components/settings/SettingsModule';
import { TauriService } from './services/tauri';

import { ServerModule } from './components/server/ServerModule';
import { InstanceModule } from './components/instances/InstanceModule';
import { SplashScreen } from './components/layout/SplashScreen';
import { ErrorBoundary } from './components/common/ErrorBoundary';
import { InlineError } from './components/common/InlineError';

import { Sparkles, CheckCircle2, AlertTriangle } from 'lucide-react';

/**
 * Names used by the top-level boundaries so a crash report says which module
 * failed instead of "something went wrong".
 */
const MODULE_LABELS: Record<ActiveTab, string> = {
  PROFILES: 'Mod Profiles',
  MOD_LIST: 'Mod List',
  SERVERS: 'Server Suite',
  MERGER: 'Mod Merger',
  MONITOR: 'Sandbox Monitor',
  SETTINGS: 'App Settings',
};

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<ActiveTab>('PROFILES');
  const [conflicts, setConflicts] = useState<VfsConflict[]>([]);
  const [rules, setRules] = useState<PolyfillRule[]>(DEFAULT_POLYFILL_RULES);
  const [mods, setMods] = useState<ModInfo[]>([]);
  const [activeProfile, setActiveProfile] = useState<AppProfile | null>(null);
  const [openedPackageFolder, setOpenedPackageFolder] = useState<string | null>(null);
  const [errorCards, setErrorCards] = useState<TranslatedErrorCard[]>([]);
  const [autoMergeResultModal, setAutoMergeResultModal] = useState<{
    patch_mod_dir: string;
    total_conflicts: number;
    files_written: number;
    polyfills_injected: number;
  } | null>(null);
  /** Inline replacement for the old alert() calls; null renders nothing. */
  const [appError, setAppError] = useState<string | null>(null);

  // Loading Screen & Startup State
  const [isInitializing, setIsInitializing] = useState<boolean>(true);
  const [initStatusMessage, setInitStatusMessage] = useState<string>('Starting PZ Mod Studio...');
  const [initProgress, setInitProgress] = useState<number>(15);

  // Studio Directory Paths State (Persistent Profile)
  const [paths, setPaths] = useState<StudioPathsUI>({
    pz_install_dir: '',
    workshop_dir: '',
    user_zomboid_dir: '',
    mod_list_ini_path: '',
    is_valid: false,
  });

  const refreshActiveProfile = useCallback(async (customUserDir?: string): Promise<AppProfile | null> => {
    const userDir = customUserDir || paths.user_zomboid_dir;
    if (userDir) {
      try {
        const list: AppProfile[] = await TauriService.listInstances(userDir);
        const active = list.find((i) => i.is_active) || null;
        setActiveProfile(active);
        return active;
      } catch (err) {
        console.error('Error listing profiles:', err);
        return null;
      }
    }
    return null;
  }, [paths.user_zomboid_dir]);

  // Load saved profile or auto-detect paths from Rust backend on initial load
  useEffect(() => {
    const initPaths = async () => {
      try {
        setInitStatusMessage('[VFS] Verifying Project Zomboid and Steam Workshop paths...');
        setInitProgress(25);
        const savedProfile = await TauriService.loadSavedPathsProfile();
        setPaths(savedProfile);

        if (savedProfile.is_valid) {
          setInitStatusMessage('[Profiles] Reading ModListData.ini and active profiles...');
          setInitProgress(50);
          const active = await refreshActiveProfile(savedProfile.user_zomboid_dir);

          setInitStatusMessage('[Workshop] Scanning mod directories on disk...');
          setInitProgress(75);
          // Scan all subscribed Workshop & local mods on disk (preserving saved absolute order)
          const allSubscribedMods = await TauriService.scanAllInstalledMods(savedProfile);
          if (allSubscribedMods.length > 0) {
            setMods([...allSubscribedMods]);
            setInitStatusMessage(`[Workshop] ${allSubscribedMods.length} mods loaded and indexed.`);
          }
          setConflicts([]);
          setInitProgress(90);

          // Determine initial startup screen based on settings and active profile presence
          const startupBehavior = localStorage.getItem('pz_startup_behavior') || 'SHOW_STARTUP_SCREEN';
          if (!active) {
            setActiveTab('PROFILES');
          } else if (startupBehavior === 'OPEN_LAST_PROFILE') {
            setActiveTab('MOD_LIST');
          } else {
            setActiveTab('PROFILES');
          }
        } else {
          setInitStatusMessage('[Setup] Configuring initial workspace...');
          setInitProgress(85);
        }

        setInitStatusMessage('[Engine] Initialization complete! Opening environment...');
        setInitProgress(100);
      } catch (e) {
        console.error('Initialization error:', e);
      } finally {
        setTimeout(() => {
          setIsInitializing(false);
        }, 600);
      }
    };
    initPaths();
  }, []);


  // Listen to realtime sandbox logs & error cards from Rust watcher
  useEffect(() => {
    let unlistenErrorCards: (() => void) | undefined;

    TauriService.listenSandboxErrorCards((card) => {
      setErrorCards((prev) => {
        // If suggested polyfill rule is ALREADY active, skip card creation!
        if (card.polyfill_rule_id_suggestion && rules.some((r) => r.id === card.polyfill_rule_id_suggestion && r.enabled)) {
          return prev;
        }
        // Deduplicate cards by title or rule suggestion so 32 identical cards never stack!
        const isDuplicate = prev.some((c) => c.title === card.title || (c.polyfill_rule_id_suggestion && c.polyfill_rule_id_suggestion === card.polyfill_rule_id_suggestion));
        if (isDuplicate) return prev;

        return [card, ...prev];
      });
    }).then((unlisten) => {
      unlistenErrorCards = unlisten;
    });

    return () => {
      if (unlistenErrorCards) unlistenErrorCards();
    };
  }, [rules]);



  const handleRescan = async () => {
    if (paths.is_valid) {
      const scannedConflicts = await TauriService.scanConflicts(paths);
      setConflicts(scannedConflicts);
      const allSubscribedMods = await TauriService.scanAllInstalledMods(paths);
      if (allSubscribedMods.length > 0) {
        setMods([...allSubscribedMods]);
      }
    }
  };

  const handleRefreshMods = async () => {
    if (paths.is_valid) {
      const allSubscribedMods = await TauriService.scanAllInstalledMods(paths);
      setMods([...allSubscribedMods]);
    }
  };

  // Auto-refresh mod list when switching to MOD_LIST tab to reflect external additions/unsubscriptions
  useEffect(() => {
    if (activeTab === 'MOD_LIST' && paths.is_valid) {
      handleRefreshMods();
    }
  }, [activeTab]);

  const handleLoadMockups = (mockups: VfsConflict[]) => {
    setConflicts(mockups);
  };

  const handleLoadModMockups = (mockMods: ModInfo[]) => {
    setMods(mockMods);
  };

  // Auto-Merge All conflicts in memory (without premature packaging)
  const handleAutoMergeAll = async () => {
    setAppError(null);
    if (!paths.is_valid) {
      setAppError(
        'Your PZ paths are not validated yet. Open App Settings and run Auto-Detect Paths (or fix the flagged folder), then merge again.'
      );
      return;
    }

    try {
      const activePolyfillIds = rules.filter((r) => r.enabled).map((r) => r.id);
      const defaultPkgName = 'Z_PZModStudio_MergedPatch';

      const mergedFiles = conflicts.map((c) => ({
        relative_path: c.relative_path,
        content: c.merged_output || c.base_content,
      }));

      const result = await TauriService.generateMasterPatch({
        workshop_dir: paths.workshop_dir,
        pz_install_dir: paths.pz_install_dir,
        user_zomboid_dir: paths.user_zomboid_dir,
        mod_list_ini_path: paths.mod_list_ini_path,
        merged_files: mergedFiles,
        active_polyfill_ids: activePolyfillIds,
        package_folder_name: defaultPkgName,
      });

      setAutoMergeResultModal({
        patch_mod_dir: result.patch_mod_dir,
        total_conflicts: conflicts.length,
        files_written: result.files_written,
        polyfills_injected: result.polyfills_injected,
      });

      await handleRefreshMods();
    } catch (err: any) {
      setAppError(`Auto-Merge could not run: ${err?.message ? err.message : err}. No patch package was written.`);
    }
  };

  // Explicit Packaging: Generates Master Patch mod on Disk and Publishes to ModListData.ini
  const handleOptimizeAndResolve = async (packageFolderName?: string) => {
    const totalCount = conflicts.length;

    const updatedRules = rules.map((r) => ({
      ...r,
      enabled: true,
    }));
    setRules(updatedRules);

    // Call Rust to generate synthetic Master Patch mod on disk
    if (paths.is_valid && paths.user_zomboid_dir) {
      const mergedFilesPayload = conflicts.map((c) => ({
        relative_path: c.relative_path,
        content: c.merged_output || c.base_content,
      }));

      const activePolyfillIds = updatedRules.filter((r) => r.enabled).map((r) => r.id);

      const result = await TauriService.generateMasterPatch({
        workshop_dir: paths.workshop_dir,
        pz_install_dir: paths.pz_install_dir,
        user_zomboid_dir: paths.user_zomboid_dir,
        mod_list_ini_path: paths.mod_list_ini_path,
        merged_files: mergedFilesPayload,
        active_polyfill_ids: activePolyfillIds,
        package_folder_name: packageFolderName,
      });

      // Mark all conflicts as resolved
      setConflicts((prev) =>
        prev.map((c) => ({
          ...c,
          status: 'RESOLVED' as const,
        }))
      );

      // Refresh installed mods so the newly published package appears in the Mod List tab immediately
      const allSubscribedMods = await TauriService.scanAllInstalledMods(paths);
      if (allSubscribedMods.length > 0) {
        setMods([...allSubscribedMods]);
      }

      if (result.success) {
        setAutoMergeResultModal({
          patch_mod_dir: result.patch_mod_dir,
          total_conflicts: totalCount,
          files_written: result.files_written,
          polyfills_injected: result.polyfills_injected,
        });
      }
    }
  };

  const [pendingLaunchMode, setPendingLaunchMode] = useState<GameLaunchMode | null>(null);
  const [skipNoMergeWarning, setSkipNoMergeWarning] = useState<boolean>(() => {
    return localStorage.getItem('pz_skip_no_merge_warning') === 'true';
  });

  const executeLaunch = async (mode: GameLaunchMode = 'DEBUG_FULLSCREEN') => {
    setAppError(null);
    try {
      const activeModIds = mods.filter((m) => m.enabled).map((m) => m.mod_id);
      await TauriService.writeModListIni(paths.mod_list_ini_path, activeModIds);

      await TauriService.launchSandbox({
        pz_install_dir: paths.pz_install_dir,
        user_zomboid_dir: paths.user_zomboid_dir,
        test_mode: mode,
      });

      if (mode.includes('DEBUG') || mode === 'MONITORED' || mode === 'WINDOWED') {
        setActiveTab('MONITOR');
      }
    } catch (err: any) {
      setAppError(
        `Project Zomboid could not be launched: ${err?.message ? err.message : err}. Check the install path in App Settings and that no other PZ instance is running.`
      );
    }
  };

  const handleRunSandbox = async (mode: GameLaunchMode = 'DEBUG_FULLSCREEN') => {
    // Check if there is an active CLOSED/PACKAGED synthetic merge package (excluding Carrier and Bridge)
    const hasActiveClosedMergePackage = mods.some(
      (m) => m.enabled && m.mod_id.startsWith('Z_PZModStudio_') && m.is_packaged === true && m.mod_id !== 'Z_PZModStudio_Bridge' && !m.mod_id.includes('Carrier')
    );

    const openDraftPackages = mods.filter(
      (m) => m.mod_id.startsWith('Z_PZModStudio_') && m.mod_id !== 'Z_PZModStudio_Bridge' && !m.mod_id.includes('Carrier') && (m.is_packaged === false || m.mod_id === openedPackageFolder)
    );

    // If there is no active closed package (either because it is open/draft, or none is active)
    if (!hasActiveClosedMergePackage) {
      if (openDraftPackages.length > 0 || !skipNoMergeWarning) {
        setPendingLaunchMode(mode);
        return;
      }
    }

    await executeLaunch(mode);
  };

  const handleResolveConflict = async (conflictId: string, resolvedCode: string, packageFolderName?: string) => {
    setConflicts((prev) =>
      prev.map((c) =>
        c.id === conflictId ? { ...c, merged_output: resolvedCode, status: 'RESOLVED' as const } : c
      )
    );

    const targetConflict = conflicts.find((c) => c.id === conflictId);
    if (targetConflict && paths.user_zomboid_dir) {
      await TauriService.saveDraftResolution(
        paths.user_zomboid_dir,
        packageFolderName || 'Z_PZModStudio_MergedPatch',
        targetConflict.relative_path,
        resolvedCode,
        'RESOLVED'
      );
    }
  };

  const handleToggleRule = (ruleId: string) => {
    setRules((prev) =>
      prev.map((r) => (r.id === ruleId ? { ...r, enabled: !r.enabled } : r))
    );
  };

  const autoPersistProfileChanges = async (updatedMods: ModInfo[]) => {
    const userDir = paths.user_zomboid_dir || '';
    const activeModIds = updatedMods.filter((m) => m.enabled).map((m) => m.mod_id);
    const fullLoadOrder = updatedMods.map((m) => m.mod_id);

    try {
      await TauriService.saveMasterLoadOrder(userDir, fullLoadOrder, activeModIds);
    } catch (err) {
      console.warn('Auto-persisting profile load order:', err);
    }
  };

  const handleReorderMods = (newOrder: ModInfo[]) => {
    setMods([...newOrder]);
    autoPersistProfileChanges(newOrder);
  };

  const handleToggleMod = (modId: string) => {
    const target = mods.find((m) => m.mod_id === modId);
    if (!target) return;

    const isSyntheticFusionPackage =
      modId.startsWith('Z_PZModStudio_') &&
      !modId.includes('Carrier') &&
      modId !== 'Z_PZModStudio_Bridge';

    // Block activation of open / draft packages (is_packaged === false)
    if (isSyntheticFusionPackage && target.is_packaged === false) {
      return;
    }

    setMods((prev) => {
      const willEnable = !target.enabled;

      const updated = prev.map((m) => {
        if (m.mod_id === modId) {
          return { ...m, enabled: willEnable };
        }
        // If enabling a synthetic fusion package, ensure only ONE fusion package is active at a time!
        // (The Live Bridge companion mod is independent and can remain active simultaneously)
        if (
          willEnable &&
          isSyntheticFusionPackage &&
          m.mod_id.startsWith('Z_PZModStudio_') &&
          !m.mod_id.includes('Carrier') &&
          m.mod_id !== 'Z_PZModStudio_Bridge'
        ) {
          return { ...m, enabled: false };
        }
        return m;
      });

      autoPersistProfileChanges(updated);
      return [...updated];
    });
  };

  const handleApplyPresetLoadOrder = (presetLoadOrder: string[], activeIds: string[]) => {
    setMods((prev) => {
      const modMap = new Map(prev.map((m) => [m.mod_id, m]));
      const activeSet = new Set(activeIds);

      const reordered: ModInfo[] = [];
      presetLoadOrder.forEach((id) => {
        const found = modMap.get(id);
        if (found) {
          reordered.push({ ...found, enabled: activeSet.has(id) });
          modMap.delete(id);
        }
      });

      // Append remaining mods
      modMap.forEach((m) => {
        reordered.push({ ...m, enabled: activeSet.has(m.mod_id) });
      });

      autoPersistProfileChanges(reordered);
      return reordered;
    });
    refreshActiveProfile();
  };

  const handleApplyFix = async (polyfillRuleId: string) => {
    const updatedRules = rules.map((r) => (r.id === polyfillRuleId ? { ...r, enabled: true } : r));
    setRules(updatedRules);
    setErrorCards((prev) => prev.filter((card) => card.polyfill_rule_id_suggestion !== polyfillRuleId));

    if (paths.is_valid && paths.user_zomboid_dir) {
      await TauriService.generateMasterPatch({
        workshop_dir: paths.workshop_dir,
        pz_install_dir: paths.pz_install_dir,
        user_zomboid_dir: paths.user_zomboid_dir,
        mod_list_ini_path: paths.mod_list_ini_path,
        merged_files: conflicts.map((c) => ({ relative_path: c.relative_path, content: c.merged_output || c.base_content })),
        active_polyfill_ids: updatedRules.filter((r) => r.enabled).map((r) => r.id),
      });
    }
  };

  const handleClearErrorCards = () => {
    setErrorCards([]);
  };

  const handleSavePaths = async (updatedPaths: StudioPathsUI) => {
    const saved = await TauriService.savePathsProfile(updatedPaths);
    setPaths(saved);
    if (saved.is_valid) {
      await refreshActiveProfile(saved.user_zomboid_dir);
      const allSubscribedMods = await TauriService.scanAllInstalledMods(saved);
      if (allSubscribedMods.length > 0) {
        setMods([...allSubscribedMods]);
      }
    }
  };

  const handleAutoDetect = async () => {
    const autoPaths = await TauriService.getAutoPaths();
    const saved = await TauriService.savePathsProfile(autoPaths);
    setPaths(saved);
    if (saved.is_valid) {
      await refreshActiveProfile(saved.user_zomboid_dir);
      const allSubscribedMods = await TauriService.scanAllInstalledMods(saved);
      if (allSubscribedMods.length > 0) {
        setMods([...allSubscribedMods]);
      }
    }
  };

  return (
    <>
      <SplashScreen
        isLoading={isInitializing}
        statusMessage={initStatusMessage}
        progress={initProgress}
      />
      <div className="h-screen w-screen flex flex-col bg-slate-950 text-slate-100 overflow-hidden font-sans">
      {/* Studio Header */}
      <StudioHeader
        conflictCount={conflicts.length}
        polyfillCount={rules.filter((r) => r.enabled).length}
        activeProfileName={activeProfile?.name}
        onNavigateToProfiles={() => setActiveTab('PROFILES')}
        onRunSandbox={handleRunSandbox}
      />


      {/* Main Studio Body */}
      <div className="flex-1 flex overflow-hidden">
        {/* StudioSidebar */}
        <StudioSidebar
          activeTab={activeTab}
          setActiveTab={setActiveTab}
          hasActiveProfile={!!activeProfile}
          conflictCount={conflicts.length}
          errorCardCount={errorCards.length}
        />

        {/* Tab Modules — each tab is guarded so a single broken panel cannot
            take down the sidebar, the header or the user's other work. */}
        <main className="flex-1 flex flex-col min-h-0 min-w-0 overflow-hidden bg-slate-950">
          {appError && (
            <div className="px-6 pt-4 shrink-0">
              <InlineError
                title="Studio error"
                message={appError}
                onDismiss={() => setAppError(null)}
              />
            </div>
          )}

          {activeTab === 'PROFILES' && (
            <div className="flex-1 flex flex-col min-h-0 overflow-y-auto p-6 font-sans">
              <ErrorBoundary name={MODULE_LABELS.PROFILES}>
              <InstanceModule
                paths={paths}
                mods={mods}
                onApplyInstanceLoadOrder={handleApplyPresetLoadOrder}
                onProfileActivated={(inst) => {
                  setActiveProfile(inst);
                  setActiveTab('MOD_LIST');
                }}
              />
              </ErrorBoundary>
            </div>
          )}

          {activeTab === 'MOD_LIST' && (
            <ErrorBoundary name={MODULE_LABELS.MOD_LIST} onReset={handleRefreshMods}>
              <LoadOrderModule
                paths={paths}
                mods={mods}
                onReorder={handleReorderMods}
                onToggleMod={handleToggleMod}
                onRefreshMods={handleRefreshMods}
                onGoToSettings={() => setActiveTab('SETTINGS')}
                onLoadMockups={handleLoadModMockups}
                onApplyPresetLoadOrder={handleApplyPresetLoadOrder}
                openedPackageFolder={openedPackageFolder}
              />
            </ErrorBoundary>
          )}

          {activeTab === 'SERVERS' && (
            <div className="flex-1 flex flex-col min-h-0 overflow-y-auto p-6 font-sans">
              <ErrorBoundary name={MODULE_LABELS.SERVERS}>
                <ServerModule
                  paths={paths}
                  mods={mods}
                />
              </ErrorBoundary>
            </div>
          )}

          {activeTab === 'MERGER' && (
            <ErrorBoundary name={MODULE_LABELS.MERGER} onReset={handleRescan}>
              <MergerModule
                conflicts={conflicts}
                paths={paths}
                onResolveConflict={handleResolveConflict}
                onAutoMergeAll={handleAutoMergeAll}
                onOptimizeAndResolve={handleOptimizeAndResolve}
                onGoToSettings={() => setActiveTab('SETTINGS')}
                onRescan={handleRescan}
                onClearConflicts={() => setConflicts([])}
                onLoadMockups={handleLoadMockups}
                onToggleMod={handleToggleMod}
                onRefreshMods={handleRefreshMods}
                onPackageOpened={(folder) => setOpenedPackageFolder(folder)}
                onPackageClosed={() => setOpenedPackageFolder(null)}
              />
            </ErrorBoundary>
          )}

          {activeTab === 'MONITOR' && (
            <ErrorBoundary name={MODULE_LABELS.MONITOR}>
              <SandboxModule
                paths={paths}
                errorCards={errorCards}
                onApplyFix={handleApplyFix}
                onClearErrorCards={handleClearErrorCards}
                onGoToSettings={() => setActiveTab('SETTINGS')}
              />
            </ErrorBoundary>
          )}

          {activeTab === 'SETTINGS' && (
            <div className="flex-1 flex flex-col min-h-0 overflow-y-auto p-6 font-sans">
              <ErrorBoundary name={MODULE_LABELS.SETTINGS} onReset={handleAutoDetect}>
                <SettingsModule
                  paths={paths}
                  rules={rules}
                  onSavePaths={handleSavePaths}
                  onToggleRule={handleToggleRule}
                  onAutoDetect={handleAutoDetect}
                />
              </ErrorBoundary>
            </div>
          )}
        </main>
      </div>


      {/* Sleek Native Auto-Merge Completion Modal */}
      {autoMergeResultModal && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-emerald-500/40 rounded-2xl p-6 max-w-lg w-full space-y-4 shadow-2xl animate-fade-in text-slate-200">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-xl bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 flex items-center justify-center shrink-0">
                <Sparkles className="w-5 h-5 text-emerald-400" />
              </div>
              <div>
                <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
                  <span>✨ Auto-Merge Completed Successfully</span>
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">All script conflicts were resolved in the merge patch.</p>
              </div>
            </div>

            <div className="bg-slate-950 border border-slate-800 rounded-xl p-4 space-y-2.5 text-xs font-mono">
              <div className="text-emerald-400 font-bold flex items-center gap-2">
                <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                <span>Patch Generated on Disk:</span>
              </div>
              <div className="p-2 bg-slate-900 rounded border border-slate-800 text-[11px] text-slate-300 break-all select-all font-mono">
                {autoMergeResultModal.patch_mod_dir}
              </div>
              <div className="grid grid-cols-2 gap-2 pt-1 text-slate-300">
                <div className="p-2 bg-slate-900/60 rounded border border-slate-800/80">
                  <span className="text-slate-400 text-[10px] block">Conflicts Resolved</span>
                  <b className="text-emerald-400 text-sm">{autoMergeResultModal.total_conflicts}</b>
                </div>
                <div className="p-2 bg-slate-900/60 rounded border border-slate-800/80">
                  <span className="text-slate-400 text-[10px] block">Files Merged</span>
                  <b className="text-emerald-400 text-sm">{autoMergeResultModal.files_written}</b>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end pt-2">
              <button
                onClick={() => setAutoMergeResultModal(null)}
                className="px-5 py-2.5 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white text-xs font-bold rounded-lg cursor-pointer shadow-lg transition"
              >
                Accept and Continue
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Launch Warning Modal — Open Package or No Merge Package */}
      {pendingLaunchMode && (() => {
        const openPackages = mods.filter(
          (m) => m.mod_id.startsWith('Z_PZModStudio_') && m.mod_id !== 'Z_PZModStudio_Bridge' && !m.mod_id.includes('Carrier') && (m.is_packaged === false || m.mod_id === openedPackageFolder)
        );
        const isOpenPackageWarning = openPackages.length > 0;
        const openPackageNames = openPackages.map((p) => p.name || p.mod_id).join(', ');

        return (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-amber-500/50 rounded-2xl p-6 max-w-md w-full space-y-4 shadow-2xl animate-fade-in text-slate-200">
            <div className="flex items-start gap-3.5">
              <div className="w-10 h-10 rounded-xl bg-amber-500/10 text-amber-400 border border-amber-500/30 flex items-center justify-center shrink-0">
                <AlertTriangle className="w-5 h-5 text-amber-400" />
              </div>
              <div className="space-y-1">
                <h3 className="text-base font-bold text-slate-100">
                  {isOpenPackageWarning ? 'Open Package (Deactivated)' : 'No Active Merge Package'}
                </h3>
                <p className="text-xs text-amber-300/90 font-medium">
                  {isOpenPackageWarning
                    ? <>The package <code className="text-amber-200 bg-amber-950/60 px-1 rounded">{openPackageNames}</code> is open in draft mode and is currently deactivated.</>
                    : 'There is no active merge patch in your profile.'}
                </p>
              </div>
            </div>

            <p className="text-xs text-slate-300 leading-relaxed bg-slate-950/60 p-3 rounded-xl border border-slate-800">
              {isOpenPackageWarning
                ? 'An open package (draft) is deactivated to prevent runtime conflicts while editing. To have it take effect in the game, you must package / publish it in Mod Merger before starting the game.'
                : 'If your installed mods have conflicting files (Lua code collisions or shared scripts), errors, unexpected behaviors, or crashes could occur during gameplay. We suggest creating or activating a merge package in Mod Merger.'}
            </p>

            {!isOpenPackageWarning && (
              <div className="flex items-center gap-2 pt-1">
                <input
                  type="checkbox"
                  id="skipNoMergeWarningCheckbox"
                  checked={skipNoMergeWarning}
                  onChange={(e) => {
                    const checked = e.target.checked;
                    setSkipNoMergeWarning(checked);
                    localStorage.setItem('pz_skip_no_merge_warning', checked ? 'true' : 'false');
                  }}
                  className="rounded border-slate-700 text-amber-500 focus:ring-amber-500/40 bg-slate-950 cursor-pointer"
                />
                <label htmlFor="skipNoMergeWarningCheckbox" className="text-[11px] text-slate-400 select-none cursor-pointer">
                  Do not show this warning again
                </label>
              </div>
            )}

            <div className="flex items-center justify-end gap-2 pt-2 border-t border-slate-800">
              <button
                onClick={() => {
                  setPendingLaunchMode(null);
                  setActiveTab('MERGER');
                }}
                className="px-4 py-2 bg-purple-600 hover:bg-purple-500 text-white text-xs font-bold rounded-lg transition cursor-pointer shadow"
              >
                Go to Mod Merger
              </button>
              <button
                onClick={() => {
                  const mode = pendingLaunchMode;
                  setPendingLaunchMode(null);
                  executeLaunch(mode);
                }}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-bold rounded-lg transition cursor-pointer"
              >
                {isOpenPackageWarning ? 'Launch without package' : 'Launch anyway'}
              </button>
            </div>
          </div>
        </div>
        );
      })()}
    </div>
    </>
  );
};

export default App;
