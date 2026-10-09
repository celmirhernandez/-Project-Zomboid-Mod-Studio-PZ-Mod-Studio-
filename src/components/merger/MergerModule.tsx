import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { VfsConflict } from '../../types';
import { StudioPathsUI } from '../settings/SettingsModule';
import { TauriService, MasterPatchStatusInfoUI, MergedPackageInfoUI } from '../../services/tauri';
import {
  GitCompare,
  CheckCircle2,
  AlertTriangle,
  FileCode,
  Check,
  EyeOff,
  Eye,
  Layers,
  ShieldCheck,
  FolderX,
  AlertCircle,
  Wand2,
  GripHorizontal,
  Sparkle,
  PackageCheck,
  Package,
  Unlock,
  RefreshCw,
  Plus,
  Edit2,
  Trash2,
  Box,
  RotateCcw,
  Code2,
  Download,
  Upload,
  FolderOpen,
  ArrowLeft,
  Undo2,
  Minus,
  Lock,
} from 'lucide-react';
import Editor from '@monaco-editor/react';
import { InlineError } from '../common/InlineError';

interface MergerModuleProps {
  conflicts: VfsConflict[];
  paths: StudioPathsUI;
  onResolveConflict: (conflictId: string, resolvedCode: string, packageFolderName?: string) => void;
  onAutoMergeAll?: (packageFolderName?: string) => void;
  onOptimizeAndResolve: (packageFolderName?: string) => void;
  onGoToSettings: () => void;
  onRescan: () => Promise<void> | void;
  onClearConflicts?: () => void;
  onLoadMockups?: (mockups: VfsConflict[]) => void;
  onToggleMod?: (modId: string) => void;
  onRefreshMods?: () => Promise<void> | void;
  onPackageOpened?: (folderName: string) => void;
  onPackageClosed?: () => void;
}

export const MergerModule: React.FC<MergerModuleProps> = ({
  conflicts,
  paths,
  onResolveConflict,
  onAutoMergeAll,
  onOptimizeAndResolve,
  onGoToSettings,
  onRescan,
  onClearConflicts,
  onLoadMockups: _onLoadMockups,
  onToggleMod,
  onRefreshMods,
  onPackageOpened,
  onPackageClosed,
}) => {
  const [selectedConflictId, setSelectedConflictId] = useState<string>(conflicts[0]?.id || '');
  const [filterNoise, setFilterNoise] = useState<boolean>(false);
  const [isRescanning, setIsRescanning] = useState<boolean>(false);
  const [isCleaning, setIsCleaning] = useState<boolean>(false);
  const [isGenerating, setIsGenerating] = useState<boolean>(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  /** Inline replacement for the old alert() calls; null renders nothing. */
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [patchStatus, setPatchStatus] = useState<MasterPatchStatusInfoUI | null>(null);

  // Multi-Package State
  const packageSwitchIdRef = useRef(0);
  const [packages, setPackages] = useState<MergedPackageInfoUI[]>([]);
  const [selectedPackageFolder, setSelectedPackageFolder] = useState<string>('Z_PZModStudio_MergedPatch');
  const [isPackageOpened, setIsPackageOpened] = useState<boolean>(false);
  const [isCreateModalOpen, setIsCreateModalOpen] = useState<boolean>(false);
  const [newPackageName, setNewPackageName] = useState<string>('');
  const [newPackageDescription, setNewPackageDescription] = useState<string>('');
  const [editingPackageFolder, setEditingPackageFolder] = useState<string | null>(null);
  const [editingPackageName, setEditingPackageName] = useState<string>('');
  const [editingPackageDescription, setEditingPackageDescription] = useState<string>('');
  const [isUnpackageModalOpen, setIsUnpackageModalOpen] = useState<boolean>(false);

  // Toast Helper
  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 4000);
  };

  const fetchPackages = useCallback(async () => {
    const list = await TauriService.listMergedPackages(paths.user_zomboid_dir || '', paths.mod_list_ini_path || '');
    setPackages(list);
    if (list.length > 0 && !list.some((p) => p.folder_name === selectedPackageFolder)) {
      setSelectedPackageFolder(list[0].folder_name);
    }
  }, [paths.user_zomboid_dir, paths.mod_list_ini_path, selectedPackageFolder]);

  const fetchPatchStatus = useCallback(async () => {
    if (selectedPackageFolder) {
      const status = await TauriService.getMasterPatchStatus(paths.user_zomboid_dir || '', paths.mod_list_ini_path || '', selectedPackageFolder);
      setPatchStatus(status);
    }
  }, [paths.user_zomboid_dir, paths.mod_list_ini_path, selectedPackageFolder]);

  useEffect(() => {
    fetchPackages();
  }, [fetchPackages, conflicts]);

  const handleOpenPackage = async (folderName: string) => {
    const switchId = ++packageSwitchIdRef.current;
    setSelectedPackageFolder(folderName);
    setIsPackageOpened(true);
    onPackageOpened?.(folderName);
    // Fetch with explicit folderName to avoid stale closure
    try {
      const status = await TauriService.getMasterPatchStatus(
        paths.user_zomboid_dir || '', paths.mod_list_ini_path || '', folderName
      );
      // Guard: only apply if this is still the latest switch
      if (packageSwitchIdRef.current !== switchId) return;
      setPatchStatus(status);
    } catch (err) {
      console.error('Error fetching patch status:', err);
    }
    if (packageSwitchIdRef.current !== switchId) return;
    if (onRescan) await onRescan();
  };

  const handleClosePackage = () => {
    setIsPackageOpened(false);
    onPackageClosed?.();
    if (onClearConflicts) onClearConflicts();
  };

  const handleTogglePackageInModlist = async (pkg: MergedPackageInfoUI) => {
    if (!paths.user_zomboid_dir) return;
    const targetState = !pkg.is_active_in_modlist;
    const success = await TauriService.togglePackageInModlist(
      paths.user_zomboid_dir,
      paths.mod_list_ini_path,
      pkg.folder_name,
      targetState
    );
    if (success) {
      if (onRefreshMods) {
        await onRefreshMods();
      } else if (onToggleMod) {
        onToggleMod(pkg.folder_name);
      }
      await fetchPackages();
      if (onRescan) await onRescan();
      showToast(
        targetState
          ? `✓ Package "${pkg.display_name}" activated and VISIBLE in the mod list.`
          : `✓ Package "${pkg.display_name}" HIDDEN from the mod list.`
      );
    }
  };

  const handleUnmergeAll = async () => {
    if (confirm(`Unmerge all resolutions in this package? The original base code of each mod will be restored.`)) {
      if (paths.user_zomboid_dir && selectedPackageFolder) {
        await TauriService.clearDraftResolutions(paths.user_zomboid_dir, selectedPackageFolder);
      }
      if (currentConflict) {
        setEditorContent(currentConflict.base_content);
        setIsManuallyEdited(false);
      }
      if (onRescan) await onRescan();
      showToast(`✨ All package merges have been reverted.`);
    }
  };

  const handleUnmergeSingleConflict = async () => {
    if (!currentConflict || !paths.user_zomboid_dir || !selectedPackageFolder) return;
    setEditorContent(currentConflict.base_content);
    setIsManuallyEdited(false);
    await TauriService.saveDraftResolution(
      paths.user_zomboid_dir,
      selectedPackageFolder,
      currentConflict.relative_path,
      currentConflict.base_content,
      'PENDING'
    );
    if (onRescan) await onRescan();
    showToast(`✓ Merge reverted for "${currentConflict.relative_path}".`);
  };

  // Noise Filtering logic
  const filteredConflicts = useMemo(() => {
    return conflicts.filter((c) => {
      if (filterNoise && c.is_identical_noise) {
        return false;
      }
      return true;
    });
  }, [conflicts, filterNoise]);

  const currentConflict = useMemo(() => {
    const found = conflicts.find((c) => c.id === selectedConflictId);
    if (found) return found;
    return filteredConflicts[0] || conflicts[0];
  }, [conflicts, filteredConflicts, selectedConflictId]);

  // Vertical resizable split panel height percentage (Top carousel vs Bottom Monaco editor)
  const [topHeightPercent, setTopHeightPercent] = useState<number>(50);
  const [isResizing, setIsResizing] = useState<boolean>(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Editor content state
  const [editorContent, setEditorContent] = useState<string>(
    currentConflict?.merged_output || currentConflict?.base_content || ''
  );
  const [isManuallyEdited, setIsManuallyEdited] = useState<boolean>(false);

  useEffect(() => {
    if (currentConflict) {
      setEditorContent(currentConflict.merged_output || currentConflict.base_content || '');
      setIsManuallyEdited(false);
    }
  }, [currentConflict?.id, currentConflict?.merged_output]);

  const hasAnyMergedConflicts = useMemo(() => {
    return conflicts.some((c) => c.status === 'RESOLVED' || c.status === 'AUTO_MERGED');
  }, [conflicts]);

  const hasPendingConflicts = useMemo(() => {
    return conflicts.some((c) => c.status === 'PENDING' || !c.status);
  }, [conflicts]);

  const isCurrentConflictUnmerged = useMemo(() => {
    if (!currentConflict) return true;
    const isPending = currentConflict.status === 'PENDING';
    const isBaseCode = !currentConflict.merged_output || editorContent === currentConflict.base_content;
    return isPending && isBaseCode && !isManuallyEdited;
  }, [currentConflict, editorContent, isManuallyEdited]);

  const isCurrentConflictSaved = useMemo(() => {
    if (!currentConflict) return true;
    const isResolved = currentConflict.status === 'RESOLVED';
    const matchesOutput = editorContent === (currentConflict.merged_output || currentConflict.base_content);
    return isResolved && matchesOutput && !isManuallyEdited;
  }, [currentConflict, editorContent, isManuallyEdited]);

  const isCurrentConflictAstApplied = useMemo(() => {
    if (!currentConflict) return true;
    const isMergedStatus = currentConflict.status === 'RESOLVED' || currentConflict.status === 'AUTO_MERGED';
    const targetAst = currentConflict.auto_ast_output || currentConflict.merged_output || currentConflict.base_content;
    const matchesAst = editorContent === targetAst;
    return isMergedStatus && matchesAst && !isManuallyEdited;
  }, [currentConflict, editorContent, isManuallyEdited]);

  const handleCreatePackageSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newPackageName.trim()) return;
    const pkg = await TauriService.createMergedPackage(
      paths.user_zomboid_dir,
      paths.mod_list_ini_path,
      newPackageName.trim(),
      newPackageDescription.trim() || undefined
    );
    if (pkg) {
      setNewPackageName('');
      setNewPackageDescription('');
      setIsCreateModalOpen(false);
      await fetchPackages();
      setSelectedPackageFolder(pkg.folder_name);
      setIsPackageOpened(true);
      if (onRefreshMods) await onRefreshMods();
      if (onRescan) await onRescan();
      showToast(`New package "${pkg.display_name}" created in DRAFT. Ready to merge.`);
    }
  };

  const handleRenamePackageSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingPackageFolder || !editingPackageName.trim()) return;
    if (editingPackageFolder === 'Z_PZModStudio_Bridge') {
      setErrorMessage('The Live Bridge mod is an official system module and cannot be renamed.');
      setEditingPackageFolder(null);
      return;
    }
    const pkg = await TauriService.renameMergedPackage(
      paths.user_zomboid_dir,
      paths.mod_list_ini_path,
      editingPackageFolder,
      editingPackageName.trim(),
      editingPackageDescription.trim() || undefined
    );
    if (pkg) {
      setEditingPackageFolder(null);
      setEditingPackageName('');
      setEditingPackageDescription('');
      await fetchPackages();
      setSelectedPackageFolder(pkg.folder_name);
      if (onRefreshMods) await onRefreshMods();
      if (onRescan) await onRescan();
      showToast(`Package updated to "${pkg.display_name}".`);
    }
  };

  const handleDeletePackage = async (folderName: string) => {
    if (folderName === 'Z_PZModStudio_Bridge') {
      setErrorMessage('The Live Bridge mod is a protected official system module and cannot be deleted.');
      return;
    }
    if (confirm(`Are you sure you want to delete the package "${folderName}" from disk?`)) {
      await TauriService.deleteMergedPackage(paths.user_zomboid_dir, paths.mod_list_ini_path, folderName);
      if (paths.user_zomboid_dir) {
        const remainingPackages = await TauriService.listMergedPackages(paths.user_zomboid_dir, paths.mod_list_ini_path);
        setPackages(remainingPackages);
        if (remainingPackages.length > 0) {
          setSelectedPackageFolder(remainingPackages[0].folder_name);
        } else {
          setSelectedPackageFolder('');
          setIsPackageOpened(false);
          if (onClearConflicts) onClearConflicts();
        }
      }
      if (onRefreshMods) await onRefreshMods();
      showToast(`Package "${folderName}" deleted.`);
    }
  };

  const handleExportPackage = async () => {
    try {
      const cleanName = selectedPackageFolder.replace('Z_PZModStudio_', '').toLowerCase();
      const filePath = await TauriService.pickSaveFile(
        `${cleanName}_package.pzmerge`,
        'PZ Mod Studio Merged Package (.pzmerge)',
        'pzmerge'
      );
      if (filePath) {
        await TauriService.exportMergedPackage(paths.user_zomboid_dir, selectedPackageFolder, filePath);
        showToast(`✨ Package successfully saved at: ${filePath}`);
      }
    } catch (err: any) {
      setErrorMessage(`Could not export the package: ${err?.message ? err.message : err}`);
    }
  };

  const handleImportPackage = async () => {
    try {
      const filePath = await TauriService.pickOpenFile(
        'PZ Mod Studio Merged Package (.pzmerge)',
        'pzmerge'
      );
      if (filePath) {
        const imported = await TauriService.importMergedPackage(paths.user_zomboid_dir, paths.mod_list_ini_path, filePath);
        if (imported) {
          await fetchPackages();
          setSelectedPackageFolder(imported.folder_name);
          setIsPackageOpened(true);
          if (onRefreshMods) await onRefreshMods();
          if (onRescan) await onRescan();
          showToast(`✨ Package '${imported.display_name}' loaded and imported successfully.`);
        }
      }
    } catch (err: any) {
      setErrorMessage(`Could not import that package: ${err?.message ? err.message : err}`);
    }
  };

  const handleOpenPhysicalFolder = async () => {
    if (paths.user_zomboid_dir && selectedPackageFolder) {
      await TauriService.openPackageFolder(paths.user_zomboid_dir, selectedPackageFolder);
    }
  };

  const handleCleanMasterPatch = async () => {
    setIsCleaning(true);
    try {
      await TauriService.cleanMasterPatch({
        workshop_dir: paths.workshop_dir,
        pz_install_dir: paths.pz_install_dir,
        user_zomboid_dir: paths.user_zomboid_dir,
        mod_list_ini_path: paths.mod_list_ini_path,
        package_folder_name: selectedPackageFolder,
      });
      showToast(`Package "${selectedPackageFolder}" unpacked. Ready to edit.`);
      await fetchPatchStatus();
      await fetchPackages();
      if (onRefreshMods) await onRefreshMods();
      await onRescan();
    } catch (err) {
      console.error('Clean failed:', err);
    } finally {
      setIsCleaning(false);
    }
  };

  // Step 1: Auto-Merge All in memory
  const handleAutoMergeAllClick = () => {
    if (onAutoMergeAll) {
      onAutoMergeAll(selectedPackageFolder);
    }
    if (currentConflict) {
      const targetAst = currentConflict.auto_ast_output || currentConflict.merged_output || currentConflict.base_content || '';
      setEditorContent(targetAst);
      setIsManuallyEdited(false);
    }
    showToast('⚡ All conflicts have been merged with the AST engine! Review the code and edit if you wish.');
  };

  // Step 2: Save Individual Conflict File Edit
  const handleSaveSingleConflict = () => {
    if (!currentConflict) return;
    onResolveConflict(currentConflict.id, editorContent, selectedPackageFolder);
    setIsManuallyEdited(false);
    showToast(`✓ Merge saved for "${currentConflict.relative_path}". Status marked as Resolved.`);
  };

  const escapeRegExp = (str: string) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

  // Step 3: Toggle Choose Single Mod exclusively vs Reverting
  const handleToggleUseSingleMod = (modContent: string, modName: string) => {
    if (editorContent === modContent) {
      // Revert to vanilla base content
      setEditorContent(currentConflict?.base_content || '');
      setIsManuallyEdited(false);
      showToast(`↩️ Exclusive selection of "${modName}" undone. Base code restored.`);
    } else {
      setEditorContent(modContent);
      setIsManuallyEdited(true);
      showToast(`Code from "${modName}" loaded in the editor exclusively.`);
    }
  };

  // Step 3b: Toggle Append vs Remove Mod block in editor content
  const handleToggleAddMod = (modContent: string, modUid: string) => {
    const blockStart = `-- === MOD: [${modUid}] ===`;
    const blockEnd = `-- === END MOD: [${modUid}] ===`;

    if (editorContent.includes(blockStart)) {
      // REMOVE mod block
      const regex = new RegExp(`\\n*${escapeRegExp(blockStart)}[\\s\\S]*?${escapeRegExp(blockEnd)}\\n*`, 'g');
      const newContent = editorContent.replace(regex, '\n\n').trim();
      setEditorContent(newContent || currentConflict?.base_content || '');
      setIsManuallyEdited(true);
      showToast(`➖ Code from "${modUid}" removed from editor.`);
    } else {
      // ADD mod block
      const formatted = `\n\n${blockStart}\n${modContent}\n${blockEnd}`;
      setEditorContent((prev) => {
        const trimmed = prev.trim();
        if (!trimmed || (currentConflict && trimmed === currentConflict.base_content.trim())) {
          return `${currentConflict?.base_content || ''}${formatted}`;
        }
        return `${prev}${formatted}`;
      });
      setIsManuallyEdited(true);
      showToast(`➕ Code from "${modUid}" added to editor.`);
    }
  };

  // Step 4: Reset to Automatic AST Merge
  const handleResetToAutoMerge = () => {
    if (!currentConflict) return;
    const targetAst = currentConflict.auto_ast_output || currentConflict.merged_output || currentConflict.base_content || '';
    setEditorContent(targetAst);
    setIsManuallyEdited(true);
    showToast(`Automatic AST merge restored for "${currentConflict.relative_path}".`);
  };

  // Step 5: Final Packaging on Disk
  const handleGenerateMasterPatch = async () => {
    setIsGenerating(true);
    try {
      await onOptimizeAndResolve(selectedPackageFolder);
      showToast(`Package packed and successfully published in Zomboid/mods/${selectedPackageFolder}.`);
      await fetchPatchStatus();
      await fetchPackages();
      if (onRefreshMods) await onRefreshMods();
    } catch (err) {
      console.error('Generate failed:', err);
    } finally {
      setIsGenerating(false);
    }
  };

  const [scanProgress, setScanProgress] = useState<{ percent: number; text: string } | null>(null);

  const handleRescanClick = async () => {
    setIsRescanning(true);

    setScanProgress({ percent: 15, text: 'Step 1/5: Reading active mods from load list and Workshop...' });
    await new Promise((r) => setTimeout(r, 150));

    setScanProgress({ percent: 35, text: 'Step 2/5: Analyzing Lua and Script files of B41 and B42 mods...' });
    await onRescan();

    setScanProgress({ percent: 65, text: 'Step 3/5: Resolving VFS collisions and code differences...' });
    await new Promise((r) => setTimeout(r, 150));

    setScanProgress({ percent: 90, text: 'Step 4/5: Injecting real AST merges into the editor...' });
    await new Promise((r) => setTimeout(r, 150));

    setScanProgress({ percent: 100, text: 'Step 5/5: Scan 100% complete!' });

    setTimeout(() => {
      setIsRescanning(false);
      setScanProgress(null);
      showToast('Scan complete! Mods analyzed and integrated successfully.');
    }, 400);
  };

  // Handle vertical panel dragging
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  };

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isResizing || !containerRef.current) return;
      const containerRect = containerRef.current.getBoundingClientRect();
      const relativeY = e.clientY - containerRect.top;
      const newPercent = (relativeY / containerRect.height) * 100;
      setTopHeightPercent(Math.max(20, Math.min(80, newPercent)));
    },
    [isResizing]
  );

  const handleMouseUp = useCallback(() => {
    setIsResizing(false);
  }, []);

  useEffect(() => {
    if (isResizing) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    } else {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    }
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing, handleMouseMove, handleMouseUp]);

  const startLine = currentConflict?.start_line ?? 1;
  const conflictLine = currentConflict?.conflict_line ?? 1;
  const totalFileLines = currentConflict?.total_file_lines ?? currentConflict?.base_content.split('\n').length ?? 1;

  const handleSelectConflict = (c: VfsConflict) => {
    setSelectedConflictId(c.id);
    setEditorContent(c.merged_output || c.base_content);
    setIsManuallyEdited(false);
  };

  const LinedCodeSnippet: React.FC<{
    content: string;
    vanillaBaseContent: string;
    startLineNum: number;
    targetConflictLine: number;
  }> = ({ content, vanillaBaseContent, startLineNum, targetConflictLine }) => {
    const lines = content.split('\n');
    const vanillaLines = vanillaBaseContent.split('\n');
    const conflictRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
      if (conflictRef.current) {
        conflictRef.current.scrollIntoView({ block: 'center', behavior: 'smooth' });
      }
    }, [targetConflictLine, content]);

    return (
      <div className="flex-1 flex overflow-auto text-xs font-mono bg-slate-950 rounded-b-lg select-text">
        <div className="w-14 bg-slate-900/90 border-r border-slate-800 text-slate-500 py-2 px-1 text-right select-none font-mono text-[11px] shrink-0 space-y-0.5">
          {lines.map((_, idx) => {
            const currentLineNum = (startLineNum || 1) + idx;
            const isConflict = currentLineNum === targetConflictLine;
            return (
              <div
                key={idx}
                className={`leading-6 h-6 px-1 flex items-center justify-end gap-1 ${
                  isConflict ? 'text-amber-300 font-bold bg-amber-500/30 rounded' : 'hover:text-slate-300'
                }`}
              >
                {isConflict && <span className="text-[9px] text-amber-400 font-extrabold">▶</span>}
                <span>{currentLineNum}</span>
              </div>
            );
          })}
        </div>
        <div className="flex-1 py-2 overflow-x-auto">
          {lines.map((line, idx) => {
            const currentLineNum = (startLineNum || 1) + idx;
            const isConflict = currentLineNum === targetConflictLine;
            const vanillaLine = vanillaLines[idx] ?? '';
            const isModifiedFromVanilla = line.trim() !== vanillaLine.trim() && !isConflict;

            let lineStyle = 'text-slate-300 hover:bg-slate-900/40';
            if (isConflict) {
              lineStyle = 'bg-amber-500/30 text-amber-200 border-l-4 border-amber-400 font-bold shadow-md';
            } else if (isModifiedFromVanilla) {
              lineStyle = 'bg-emerald-950/40 text-emerald-300 border-l-4 border-emerald-500/80 font-medium';
            }

            return (
              <div
                key={idx}
                ref={isConflict ? conflictRef : null}
                className={`leading-6 h-6 px-3 whitespace-pre text-[12px] font-mono flex items-center transition ${lineStyle}`}
              >
                {line || ' '}
              </div>
            );
          })}
        </div>
      </div>
    );
  };

  if (!paths.is_valid) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-6 bg-slate-950 text-slate-200">
        <div className="max-w-md w-full bg-slate-900/80 border border-amber-500/40 rounded-2xl p-6 text-center space-y-4 shadow-xl">
          <div className="w-12 h-12 rounded-xl bg-amber-500/10 text-amber-400 border border-amber-500/30 flex items-center justify-center mx-auto">
            <FolderX className="w-6 h-6" />
          </div>
          <div>
            <h3 className="text-base font-bold text-slate-100">Setup Required: Game Directory</h3>
            <p className="text-xs text-slate-400 mt-1 leading-relaxed">
              Project Zomboid installation directory could not be auto-detected. Please configure your paths in App Settings to scan active mods.
            </p>
          </div>
          <button
            onClick={onGoToSettings}
            className="w-full flex items-center justify-center gap-2 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white font-medium text-xs py-2.5 rounded-lg shadow transition cursor-pointer"
          >
            Configure Directory Paths in Settings
          </button>
        </div>
      </div>
    );
  }

  const getLineSnippet = (text: string, lineIndex: number): string => {
    const lines = text.split('\n');
    return lines[lineIndex - 1]?.trim() || '(empty or end of file)';
  };

  const currentPackage = packages.find((p) => p.folder_name === selectedPackageFolder) || packages[0];
  const allResolved = conflicts.length > 0 && conflicts.every((c) => c.status === 'RESOLVED' || c.status === 'AUTO_MERGED');
  const isCurrentlyPackaged = patchStatus?.is_packaged ?? currentPackage?.is_packaged ?? false;

  return (
    <div className="flex-1 flex flex-col overflow-hidden bg-slate-950 text-slate-200 select-none">
      {toastMessage && (
        <div className="bg-gradient-to-r from-cyan-900/90 to-emerald-900/90 border-b border-cyan-500/50 text-cyan-100 text-xs px-4 py-2 flex items-center justify-between animate-fade-in shadow-md shrink-0">
          <div className="flex items-center gap-2">
            <Sparkle className="w-4 h-4 text-cyan-300 animate-spin shrink-0" />
            <span>{toastMessage}</span>
          </div>
          <button onClick={() => setToastMessage(null)} className="text-slate-400 hover:text-white text-xs">
            ✕
          </button>
        </div>
      )}

      <div className="px-4 pt-3 shrink-0">
        <InlineError
          title="Mod Merger error"
          message={errorMessage}
          onDismiss={() => setErrorMessage(null)}
        />
      </div>

      {/* LEVEL 1: Overview & Selection (When no package is opened) */}
      {!isPackageOpened ? (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Top Bar for Level 1 */}
          <div className="bg-slate-900 border-b border-slate-800 p-3.5 flex items-center justify-between gap-4 shrink-0 shadow-md">
            <div className="flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-lg bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
                <Box className="w-4 h-4" />
              </div>
              <div>
                <h2 className="text-sm font-bold text-slate-100 flex items-center gap-2">
                  <span>Merge Packages</span>
                  <span className="text-[10px] font-mono px-2 py-0.2 rounded-full bg-slate-800 text-emerald-400 border border-slate-700">
                    {packages.length} {packages.length === 1 ? 'Package' : 'Packages'}
                  </span>
                </h2>
                <p className="text-[11px] text-slate-400">
                  Select a package and enter to inspect its merged files, edit or package.
                </p>
              </div>
            </div>

            <div className="flex items-center gap-2 shrink-0">
              <button
                onClick={handleImportPackage}
                className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 text-xs font-bold rounded-lg transition shadow shrink-0 cursor-pointer"
                title="Import a saved package (.pzmerge) from disk"
              >
                <Upload className="w-3.5 h-3.5 text-cyan-400" />
                <span>Load Package (.pzmerge)</span>
              </button>

              <button
                onClick={() => setIsCreateModalOpen(true)}
                className="flex items-center gap-1.5 px-3.5 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold rounded-lg transition shadow shrink-0 cursor-pointer"
              >
                <Plus className="w-4 h-4" />
                <span>➕ New Package</span>
              </button>

              {/* Botón de Empaquetar deshabilitado en gris porque no hay paquete abierto */}
              <button
                disabled
                className="flex items-center gap-1.5 px-3.5 py-1.5 text-xs font-bold rounded-lg bg-slate-900 border border-slate-800 text-slate-600 cursor-not-allowed opacity-50 shadow shrink-0"
                title="You must open / enter a package first to be able to scan and package"
              >
                <PackageCheck className="w-4 h-4 text-slate-600" />
                <span>📦 Package & Publish</span>
              </button>
            </div>
          </div>

          {/* Level 1 Body: Cards or Empty State */}
          {packages.length === 0 ? (
            <div className="flex-1 flex flex-col items-center justify-center p-8 text-center space-y-4">
              <div className="w-16 h-16 rounded-2xl bg-slate-900 border border-slate-800 flex items-center justify-center text-slate-500 shadow-inner">
                <Box className="w-8 h-8 text-emerald-400" />
              </div>
              <div className="space-y-1">
                <h3 className="text-base font-bold text-slate-200">No merge packages created</h3>
                <p className="text-xs text-slate-400 max-w-sm leading-relaxed">
                  To scan conflicts and merge mods, create a merge package first or load an existing .pzmerge file.
                </p>
              </div>
              <button
                onClick={() => setIsCreateModalOpen(true)}
                className="px-5 py-2.5 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white text-xs font-bold rounded-xl transition shadow-lg flex items-center gap-2 cursor-pointer"
              >
                <Plus className="w-4 h-4" />
                <span>➕ Create First Merge Package</span>
              </button>
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto p-6 space-y-4 max-w-5xl w-full mx-auto">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {packages.map((pkg) => {
                  const isSelected = pkg.folder_name === selectedPackageFolder;
                  return (
                    <div
                      key={pkg.folder_name}
                      onClick={() => {
                        setSelectedPackageFolder(pkg.folder_name);
                        fetchPatchStatus();
                      }}
                      className={`p-5 rounded-2xl border transition shadow-lg flex flex-col justify-between space-y-4 cursor-pointer ${
                        isSelected
                          ? 'bg-slate-900/95 border-emerald-500/70 ring-1 ring-emerald-500/30'
                          : 'bg-slate-900/50 border-slate-800 hover:border-slate-700 hover:bg-slate-900/80'
                      }`}
                    >
                      <div className="space-y-3">
                        {/* Row 1: Icon, Full Display Name, and Top-Right Action Icons */}
                        <div className="flex items-start justify-between gap-3">
                          <div className="flex items-center gap-3 min-w-0 flex-1">
                            <div className={`w-10 h-10 rounded-xl flex items-center justify-center border shrink-0 ${
                              pkg.is_companion_bridge
                                ? 'bg-cyan-500/10 text-cyan-400 border-cyan-500/30'
                                : pkg.is_packaged
                                ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                                : 'bg-slate-800 text-slate-400 border-slate-700'
                            }`}>
                              <Package className="w-5 h-5" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <h3 className="text-sm font-bold text-slate-100 leading-snug break-words">
                                {pkg.display_name}
                              </h3>
                              <div className="text-[11px] text-slate-400 font-mono mt-0.5 break-all">
                                {pkg.folder_name}
                              </div>
                            </div>
                          </div>

                          {/* Quick Actions (Rename / Delete or Locked System Badge) */}
                          {!pkg.is_companion_bridge ? (
                            <div className="flex items-center gap-1 shrink-0">
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setEditingPackageFolder(pkg.folder_name);
                                  setEditingPackageName(pkg.display_name);
                                  setEditingPackageDescription(pkg.description || '');
                                }}
                                className="p-1.5 hover:text-cyan-300 hover:bg-slate-800 rounded-lg transition text-slate-400 cursor-pointer"
                                title="Edit package name and description"
                              >
                                <Edit2 className="w-3.5 h-3.5" />
                              </button>

                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleDeletePackage(pkg.folder_name);
                                }}
                                className="p-1.5 hover:text-red-400 hover:bg-rose-950/40 rounded-lg transition text-slate-400 cursor-pointer"
                                title="Delete package from disk"
                              >
                                <Trash2 className="w-3.5 h-3.5" />
                              </button>
                            </div>
                          ) : (
                            <div
                              className="flex items-center gap-1 px-2 py-0.5 bg-slate-950 border border-slate-800 rounded-lg text-[10px] text-cyan-400/80 font-mono select-none"
                              title="Integrated system Companion Mod (Protected against editing or deletion)"
                            >
                              <Lock className="w-2.5 h-2.5" />
                              <span>System</span>
                            </div>
                          )}
                        </div>

                        {/* Package Description */}
                        {pkg.description && (
                          <p className="text-xs text-slate-300 leading-relaxed line-clamp-2 bg-slate-950/40 p-2 rounded-lg border border-slate-800/60 break-words">
                            {pkg.description}
                          </p>
                        )}

                        {/* Row 2: Status Badges and ModList Visibility */}
                        <div className="flex items-center gap-2 flex-wrap pt-1 border-t border-slate-800/40">
                          {pkg.is_companion_bridge ? (
                            <span className="text-[10px] font-bold px-2.5 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 border border-cyan-500/40">
                              Live IPC Bridge
                            </span>
                          ) : pkg.is_packaged ? (
                            <span className="text-[10px] font-bold px-2.5 py-0.5 rounded-full bg-emerald-950/90 text-emerald-300 border border-emerald-600 shadow-sm" title="Compiled and ready as a mod for Project Zomboid">
                              🟢 PACKAGED / CLOSED
                            </span>
                          ) : (
                            <span className="text-[10px] font-bold px-2.5 py-0.5 rounded-full bg-amber-950/90 text-amber-300 border border-amber-600 shadow-sm animate-pulse" title="Open package: in editing mode with draft conflict resolutions, deactivated for gameplay">
                              🟡 OPEN (Draft)
                            </span>
                          )}

                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              handleTogglePackageInModlist(pkg);
                            }}
                            className={`flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[10px] font-bold border transition shadow cursor-pointer ${
                              pkg.is_active_in_modlist
                                ? 'bg-emerald-950/80 text-emerald-300 border-emerald-500/60 hover:bg-emerald-900'
                                : 'bg-slate-800 text-slate-400 border-slate-700 hover:bg-slate-700 hover:text-slate-200'
                            }`}
                            title={
                              pkg.is_active_in_modlist
                                ? "Active and visible in the game mod list (default.txt). Click to hide."
                                : "Hidden from the game. Click to activate and show in ModList."
                            }
                          >
                            {pkg.is_active_in_modlist ? <Eye className="w-3 h-3 text-emerald-400" /> : <EyeOff className="w-3 h-3 text-slate-400" />}
                            <span>{pkg.is_active_in_modlist ? 'Visible in ModList' : 'Hidden in ModList'}</span>
                          </button>
                        </div>
                      </div>

                      <div className="flex items-center justify-between pt-3 border-t border-slate-800/80 gap-2">
                        <div className="flex items-center gap-1.5">
                          {!pkg.is_companion_bridge && (
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setSelectedPackageFolder(pkg.folder_name);
                                handleExportPackage();
                              }}
                              className="p-2 bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 rounded-lg text-xs transition cursor-pointer"
                              title="Export .pzmerge file"
                            >
                              <Download className="w-3.5 h-3.5 text-emerald-400" />
                            </button>
                          )}

                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              setSelectedPackageFolder(pkg.folder_name);
                              handleOpenPhysicalFolder();
                            }}
                            className="flex items-center gap-1.5 px-2.5 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 rounded-lg text-xs transition cursor-pointer font-medium"
                            title="Open physical folder in Windows Explorer"
                          >
                            <FolderOpen className="w-3.5 h-3.5 text-amber-400" />
                            {pkg.is_companion_bridge && <span className="text-[11px]">View Files</span>}
                          </button>
                        </div>

                        {!pkg.is_companion_bridge ? (
                          <button
                            onClick={() => handleOpenPackage(pkg.folder_name)}
                            className="px-4 py-2 bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white rounded-lg text-xs font-bold transition shadow flex items-center gap-1.5 cursor-pointer"
                          >
                            <span>🔍 Open / Enter Package</span>
                          </button>
                        ) : (
                          <div className="text-[11px] text-slate-400 flex items-center gap-1.5 font-mono px-2 py-1 bg-slate-950/60 rounded-lg border border-slate-800">
                            <span className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse"></span>
                            <span>IPC Link with the Game</span>
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      ) : (
        /* LEVEL 2: Workspace of the Opened Package */
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Top Bar for Level 2 Workspace */}
          <div className="bg-slate-900 border-b border-slate-800 p-2.5 flex items-center justify-between gap-4 shrink-0 shadow-md">
            <div className="flex items-center gap-3">
              <button
                onClick={handleClosePackage}
                className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 text-xs font-bold rounded-lg transition shadow cursor-pointer"
                title="Return to the general package list"
              >
                <ArrowLeft className="w-3.5 h-3.5 text-cyan-400" />
                <span>← Back to List</span>
              </button>

              {/* Interactive Package Switcher Dropdown */}
              <div className="flex items-center gap-1.5 bg-slate-800/90 border border-slate-700/80 rounded-lg px-2.5 py-1 shadow-sm">
                <Package className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                <span className="text-[10px] text-slate-400 font-semibold uppercase tracking-wider">Package:</span>
                <select
                  value={selectedPackageFolder}
                  onChange={(e) => handleOpenPackage(e.target.value)}
                  className="bg-transparent text-xs font-bold text-emerald-300 focus:outline-none cursor-pointer pr-1"
                >
                  {packages.filter((p) => !p.is_companion_bridge).map((p) => (
                    <option key={p.folder_name} value={p.folder_name} className="bg-slate-900 text-slate-200">
                      {p.display_name} {p.is_packaged ? '• [Packaged]' : '• [Draft / Open]'} {p.is_active_in_modlist ? '• [Visible]' : '• [Hidden]'}
                    </option>
                  ))}
                </select>
              </div>

              {currentPackage?.is_companion_bridge && (
                <span className="text-[9px] font-bold px-2 py-0.5 rounded bg-cyan-500/20 text-cyan-300 border border-cyan-500/40">
                  COMPANION BRIDGE
                </span>
              )}

              {isCurrentlyPackaged ? (
                <span className="text-[9px] font-bold px-2 py-0.5 rounded bg-emerald-950/90 text-emerald-300 border border-emerald-600 shadow-sm" title="Compiled and ready as a mod for Project Zomboid">
                  🟢 PACKAGED
                </span>
              ) : (
                <span className="text-[9px] font-bold px-2 py-0.5 rounded bg-amber-950/90 text-amber-300 border border-amber-600 shadow-sm animate-pulse" title="Package in editing mode (draft), deactivated for gameplay">
                  🟡 OPEN (Draft)
                </span>
              )}

              {/* ModList Visibility Toggle Button in Level 2 Top Bar */}
              {currentPackage && (
                <button
                  onClick={() => handleTogglePackageInModlist(currentPackage)}
                  className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-bold border transition shadow cursor-pointer ${
                    currentPackage.is_active_in_modlist
                      ? 'bg-emerald-950/80 text-emerald-300 border-emerald-600/70 hover:bg-emerald-900'
                      : 'bg-slate-800 text-slate-400 border-slate-700 hover:bg-slate-700 hover:text-slate-200'
                  }`}
                  title={currentPackage.is_active_in_modlist ? "The package is active and visible in default.txt. Click to hide." : "The package is hidden in default.txt. Click to activate."}
                >
                  {currentPackage.is_active_in_modlist ? <Eye className="w-3 h-3 text-emerald-400" /> : <EyeOff className="w-3 h-3 text-slate-400" />}
                  <span>{currentPackage.is_active_in_modlist ? 'Visible in ModList' : 'Hidden in ModList'}</span>
                </button>
              )}
            </div>

            <div className="flex items-center gap-2 shrink-0">
              {!isCurrentlyPackaged && (
                <>
                  <button
                    onClick={handleAutoMergeAllClick}
                    disabled={!hasPendingConflicts}
                    className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-bold rounded-lg transition shadow ${
                      !hasPendingConflicts
                        ? 'bg-slate-900 border border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                        : 'bg-emerald-950/90 hover:bg-emerald-900 text-emerald-300 border border-emerald-700/60 cursor-pointer'
                    }`}
                    title={!hasPendingConflicts ? "All conflicts in this package have already been merged and resolved" : "Applies the automatic AST engine to all unpacked conflicts"}
                  >
                    <Wand2 className={`w-3.5 h-3.5 ${!hasPendingConflicts ? 'text-slate-600' : 'text-emerald-400'}`} />
                    <span>⚡ Merge All (AST)</span>
                  </button>

                  <button
                    onClick={handleUnmergeAll}
                    disabled={!hasAnyMergedConflicts}
                    className={`flex items-center gap-1.5 px-3 py-1.5 border text-xs font-semibold rounded-lg transition shadow ${
                      !hasAnyMergedConflicts
                        ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                        : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700 cursor-pointer'
                    }`}
                    title={!hasAnyMergedConflicts ? "There are no active merges in this package to unmerge" : "Undoes all merges in this package and restores original files"}
                  >
                    <Undo2 className={`w-3.5 h-3.5 ${!hasAnyMergedConflicts ? 'text-slate-600' : 'text-amber-400'}`} />
                    <span>↩️ Unmerge All</span>
                  </button>
                </>
              )}

              <button
                onClick={() => setFilterNoise(!filterNoise)}
                className={`px-2.5 py-1.5 text-xs font-semibold rounded-lg flex items-center gap-1.5 border transition cursor-pointer shadow ${
                  filterNoise
                    ? 'bg-emerald-950/80 text-emerald-300 border-emerald-700/80 font-bold'
                    : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700'
                }`}
                title={filterNoise ? "Showing only real collisions (identical noise hidden)" : "Showing all colliding files"}
              >
                {filterNoise ? <EyeOff className="w-3.5 h-3.5 text-emerald-400" /> : <Eye className="w-3.5 h-3.5 text-slate-400" />}
                <span>{filterNoise ? 'No Duplicates' : 'All'}</span>
              </button>

              <button
                onClick={handleOpenPhysicalFolder}
                className="p-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 text-xs font-bold rounded-lg transition shadow cursor-pointer"
                title="Open physical folder in Windows"
              >
                <FolderOpen className="w-4 h-4 text-amber-400" />
              </button>

              <button
                onClick={handleExportPackage}
                className="p-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 text-xs font-bold rounded-lg transition shadow cursor-pointer"
                title="Save .pzmerge file"
              >
                <Download className="w-4 h-4 text-emerald-400" />
              </button>

              <button
                onClick={handleRescanClick}
                disabled={isCurrentlyPackaged || isRescanning}
                className={`p-1.5 border text-xs font-medium rounded-lg transition shadow ${
                  isCurrentlyPackaged || isRescanning
                    ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                    : 'bg-slate-800 hover:bg-slate-700 text-slate-200 border-slate-700 cursor-pointer'
                }`}
                title={isCurrentlyPackaged ? "The package is packaged (read-only). Unpack to rescan and edit." : "Rescan mod collisions"}
              >
                <RefreshCw className={`w-4 h-4 ${isCurrentlyPackaged ? 'text-slate-600' : 'text-cyan-400'} ${isRescanning ? 'animate-spin' : ''}`} />
              </button>

              {isCurrentlyPackaged ? (
                <button
                  onClick={() => setIsUnpackageModalOpen(true)}
                  disabled={isCleaning}
                  className="flex items-center gap-1.5 px-4 py-1.5 bg-amber-950/80 hover:bg-amber-900 text-amber-300 border border-amber-700/60 text-xs font-bold rounded-lg transition shadow cursor-pointer"
                  title="Unpacks and opens the package for further editing"
                >
                  <Unlock className="w-4 h-4 text-amber-400" />
                  <span>{isCleaning ? 'Opening...' : '🔓 Unmerge / Open to Edit'}</span>
                </button>
              ) : (
                <button
                  onClick={handleGenerateMasterPatch}
                  disabled={isGenerating}
                  className={`flex items-center gap-1.5 px-4 py-1.5 text-xs font-bold rounded-lg transition shadow-lg shrink-0 cursor-pointer ${
                    allResolved
                      ? 'bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white animate-pulse'
                      : 'bg-emerald-600 hover:bg-emerald-500 text-white'
                  }`}
                  title="Final Step: Compiles all merged files to disk and publishes the package to the Mod List"
                >
                  <PackageCheck className={`w-4 h-4 ${isGenerating ? 'animate-spin' : ''}`} />
                  <span>{isGenerating ? 'Packaging...' : '📦 Package & Publish'}</span>
                </button>
              )}
            </div>
          </div>

          {/* Read-Only Status Banner when isCurrentlyPackaged */}
          {isCurrentlyPackaged && (
            <div className="bg-emerald-950/40 border-b border-emerald-800/50 px-4 py-2 flex items-center justify-between gap-3 shrink-0 text-xs animate-fade-in">
              <div className="flex items-center gap-2 text-emerald-300">
                <PackageCheck className="w-4 h-4 text-emerald-400 shrink-0" />
                <span>
                  <b>Packaged Package (Mod Ready to Play):</b> The package is in read-only mode. All editing, unmerging, mod usage, and scanning functions are protected until you <b>Unpack it</b>.
                </span>
              </div>
              <button
                onClick={() => setIsUnpackageModalOpen(true)}
                disabled={isCleaning}
                className="px-3 py-1 bg-amber-600 hover:bg-amber-500 text-white rounded-lg font-bold text-[11px] transition shadow-md cursor-pointer shrink-0 flex items-center gap-1.5"
                title="Unpack and open for editing"
              >
                <Unlock className="w-3.5 h-3.5" />
                <span>{isCleaning ? 'Opening...' : 'Unpack to Edit'}</span>
              </button>
            </div>
          )}

          {/* Main Body: If package is closed/packaged, hide editor content until unlocked */}
          {isCurrentlyPackaged ? (
            <div className="flex-1 flex flex-col items-center justify-center p-8 bg-slate-950/80 overflow-y-auto text-slate-200 animate-fade-in">
              <div className="max-w-xl w-full bg-slate-900/90 border border-emerald-500/30 rounded-2xl p-8 shadow-2xl space-y-6 text-center">
                <div className="w-20 h-20 mx-auto rounded-3xl bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center text-emerald-400 shadow-inner">
                  <PackageCheck className="w-10 h-10" />
                </div>

                <div className="space-y-2">
                  <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full text-xs font-bold bg-emerald-950/90 text-emerald-300 border border-emerald-700/60">
                    <ShieldCheck className="w-4 h-4 text-emerald-400" />
                    <span>CLOSED AND PACKAGED PACKAGE</span>
                  </div>
                  <h2 className="text-2xl font-bold text-slate-100 font-mono">
                    {currentPackage?.display_name || selectedPackageFolder}
                  </h2>
                  <p className="text-xs text-slate-400 max-w-md mx-auto leading-relaxed">
                    This package is compiled in <b>Read Only</b> mode and ready to play. To avoid accidental discrepancies, editor content and collisions remain hidden until you decide to open it.
                  </p>
                </div>

                {/* Summary Info Cards */}
                <div className="grid grid-cols-2 gap-3 text-left">
                  <div className="bg-slate-950/90 border border-slate-800 rounded-xl p-3.5 space-y-1">
                    <span className="text-[11px] font-medium text-slate-400 flex items-center gap-1.5">
                      <FileCode className="w-3.5 h-3.5 text-cyan-400" />
                      Merged Files
                    </span>
                    <p className="text-lg font-bold text-emerald-400 font-mono">
                      {conflicts.length} {conflicts.length === 1 ? 'file' : 'files'}
                    </p>
                  </div>
                  <div className="bg-slate-950/90 border border-slate-800 rounded-xl p-3.5 space-y-1">
                    <span className="text-[11px] font-medium text-slate-400 flex items-center gap-1.5">
                      <Layers className="w-3.5 h-3.5 text-emerald-400" />
                      State in ModList
                    </span>
                    <p className="text-lg font-bold text-cyan-400">
                      {currentPackage?.is_active_in_modlist ? '✓ Active and Visible' : 'Hidden'}
                    </p>
                  </div>
                </div>

                {/* Open / Unpackage Action CTA */}
                <div className="pt-2 space-y-3">
                  <button
                    onClick={() => setIsUnpackageModalOpen(true)}
                    disabled={isCleaning}
                    className="w-full py-3.5 px-6 bg-gradient-to-r from-amber-600 via-amber-500 to-orange-600 hover:from-amber-500 hover:to-orange-500 text-white font-bold text-sm rounded-xl shadow-lg hover:shadow-amber-500/25 transition-all flex items-center justify-center gap-2.5 cursor-pointer"
                  >
                    <Unlock className="w-5 h-5 text-amber-100" />
                    <span>{isCleaning ? 'Opening package...' : '🔓 Unmerge / Open to Edit'}</span>
                  </button>
                  <p className="text-[11px] text-slate-500">
                    By opening the package you can inspect each colliding file, compare differences and modify the generated code.
                  </p>
                </div>
              </div>
            </div>
          ) : (
            /* Workspace Body */
            <div className="flex-1 flex overflow-hidden">
              <div className="w-80 border-r border-slate-800 flex flex-col bg-slate-900/50 shrink-0">
                <div className="p-3 border-b border-slate-800 flex items-center justify-between">
                  <div className="flex items-center gap-2 text-xs font-semibold text-slate-300">
                    <GitCompare className="w-4 h-4 text-emerald-400" />
                    <span>VFS Conflicts</span>
                  </div>
                  <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-slate-800 text-cyan-400 border border-slate-700">
                    {filteredConflicts.length} {filteredConflicts.length === 1 ? 'file' : 'files'}
                  </span>
                </div>

                {isRescanning && scanProgress && (
                  <div className="p-3 bg-cyan-950/80 border-b border-cyan-700/60 space-y-2 animate-fade-in shadow-lg">
                    <div className="flex items-center justify-between text-[11px] font-mono font-bold text-cyan-300">
                      <span className="flex items-center gap-1.5 truncate">
                        <RefreshCw className="w-3.5 h-3.5 text-cyan-400 animate-spin shrink-0" />
                        <span className="truncate">{scanProgress.text}</span>
                      </span>
                      <span className="shrink-0">{scanProgress.percent}%</span>
                    </div>
                    <div className="w-full bg-slate-900 rounded-full h-1.5 overflow-hidden border border-cyan-800">
                      <div
                        className="bg-gradient-to-r from-cyan-500 to-emerald-400 h-full transition-all duration-300 rounded-full"
                        style={{ width: `${scanProgress.percent}%` }}
                      />
                    </div>
                  </div>
                )}

                <div className="flex-1 overflow-y-auto p-2 space-y-1">
                  {filteredConflicts.length === 0 ? (
                    <div className="p-6 text-center text-xs text-slate-500 space-y-2">
                      <ShieldCheck className="w-8 h-8 text-emerald-500/50 mx-auto" />
                      <p>No pending collisions under current filter.</p>
                    </div>
                  ) : (
                    filteredConflicts.map((c) => {
                      const isSelected = c.id === selectedConflictId;
                      const isResolved = c.status === 'RESOLVED';
                      const isAutoMerged = c.status === 'AUTO_MERGED';

                      return (
                        <div
                          key={c.id}
                          onClick={() => handleSelectConflict(c)}
                          className={`p-2.5 rounded-lg border text-xs cursor-pointer transition-all ${
                            isSelected
                              ? 'bg-slate-800 border-emerald-500/70 shadow-sm ring-1 ring-emerald-500/30'
                              : 'bg-slate-900/80 border-slate-800 hover:border-slate-700'
                          }`}
                        >
                          <div className="flex items-center justify-between mb-1">
                            <span className="font-mono text-[10px] px-1.5 py-0.5 rounded bg-slate-950 text-slate-400 border border-slate-800">
                              {c.file_type}
                            </span>
                            {isResolved ? (
                              <span className="flex items-center gap-1 text-[10px] text-emerald-400 font-bold bg-emerald-950/80 px-1.5 py-0.5 rounded border border-emerald-800/80">
                                <CheckCircle2 className="w-3 h-3 text-emerald-400" /> Resolved
                              </span>
                            ) : isAutoMerged ? (
                              <span className="flex items-center gap-1 text-[10px] text-cyan-300 font-bold bg-cyan-950/80 px-1.5 py-0.5 rounded border border-cyan-800/80">
                                <Sparkle className="w-3 h-3 text-cyan-400" /> Auto-Merged
                              </span>
                            ) : (
                              <span className="flex items-center gap-1 text-[10px] text-amber-400 font-medium bg-amber-950/60 px-1.5 py-0.5 rounded border border-amber-800/60">
                                <AlertTriangle className="w-3 h-3 text-amber-400" /> Needs Review
                              </span>
                            )}
                          </div>
                          <div className="font-mono text-slate-200 font-semibold truncate" title={c.relative_path}>
                            {c.relative_path.split('/').pop()}
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>

              {currentConflict ? (
                <div ref={containerRef} className="flex-1 flex flex-col overflow-hidden min-w-0">
                  <div
                    style={{ height: `${topHeightPercent}%` }}
                    className="flex flex-col bg-slate-900/80 p-3 min-h-0 overflow-hidden"
                  >
                    {/* Header Toolbar */}
                    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-800 pb-2 mb-2 shrink-0">
                      <div className="flex items-center gap-2 overflow-hidden max-w-xl">
                        <FileCode className="w-4 h-4 text-cyan-400 shrink-0" />
                        <span className="text-xs font-bold text-slate-200 truncate">
                          Colliding File: <code className="text-emerald-400 font-mono">{currentConflict.relative_path}</code>
                        </span>
                      </div>

                      <div className="flex items-center gap-2 shrink-0">
                        <span className="flex items-center gap-1 text-[10px] font-mono px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-300 border border-amber-500/40 font-bold">
                          <AlertCircle className="w-3 h-3 text-amber-400" />
                          Line #{conflictLine}
                        </span>

                        <span className="flex items-center gap-1 text-[10px] font-mono px-2 py-0.5 rounded-full bg-slate-950 text-slate-400 border border-slate-800">
                          <Layers className="w-3 h-3 text-cyan-400" />
                          {currentConflict.competing_mods.length + 1} Sources
                        </span>
                      </div>
                    </div>

                    {/* Structured Conflict Comparison Card */}
                    <div className="bg-slate-950/90 border border-amber-500/40 rounded-xl p-2.5 mb-2 shrink-0 space-y-2 shadow-md">
                      <div className="flex items-center justify-between text-xs">
                        <div className="flex items-center gap-2 text-amber-300 font-bold">
                          <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
                          <span>Discrepancy detected at Line #{conflictLine}</span>
                        </div>
                        <span className="text-[10px] font-mono text-slate-400">
                          {currentConflict.competing_mods.length} mod versions modify this line
                        </span>
                      </div>

                      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
                        {/* Card: Vanilla Base Game */}
                        <div className="bg-slate-900/90 border border-slate-800 rounded-lg p-2 flex flex-col justify-between gap-1 shadow-inner">
                          <div className="flex items-center justify-between text-[11px]">
                            <span className="font-semibold text-slate-300">Vanilla Base Game</span>
                            <span className="font-mono text-[10px] text-slate-500 bg-slate-950 px-1.5 py-0.2 rounded border border-slate-800">
                              L#{conflictLine}
                            </span>
                          </div>
                          <div
                            className="font-mono text-[11px] text-slate-300 bg-slate-950/90 p-1.5 rounded border border-slate-800/80 truncate select-all"
                            title={getLineSnippet(currentConflict.base_content, conflictLine)}
                          >
                            {getLineSnippet(currentConflict.base_content, conflictLine) || <span className="text-slate-600 italic">(file not present in base game)</span>}
                          </div>
                        </div>

                        {/* Cards: Competing Mods */}
                        {currentConflict.competing_mods.map((mod, idx) => (
                          <div
                            key={idx}
                            className="bg-slate-900/90 border border-emerald-900/50 rounded-lg p-2 flex flex-col justify-between gap-1 shadow-inner"
                          >
                            <div className="flex items-center justify-between text-[11px] gap-2">
                              <span className="font-semibold text-emerald-400 truncate max-w-[200px]" title={mod.mod_name}>
                                {mod.mod_name}
                              </span>
                              <span className="font-mono text-[10px] text-emerald-400/80 bg-emerald-950/70 px-1.5 py-0.2 rounded border border-emerald-800/50 shrink-0">
                                L#{conflictLine}
                              </span>
                            </div>
                            <div
                              className="font-mono text-[11px] text-emerald-300 bg-emerald-950/40 p-1.5 rounded border border-emerald-800/40 truncate select-all"
                              title={getLineSnippet(mod.content, conflictLine)}
                            >
                              {getLineSnippet(mod.content, conflictLine) || <span className="text-slate-600 italic">(empty)</span>}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>

                    {/* Horizontal Code Panels Carousel */}
                    <div className="flex-1 flex gap-3 overflow-x-auto min-h-0">
                      {/* Panel A: Vanilla Base */}
                      <div className="w-96 min-w-96 bg-slate-950 border border-slate-800 rounded-lg flex flex-col shadow">
                        <div className="px-3 py-1.5 bg-slate-900/90 border-b border-slate-800 flex items-center justify-between gap-2">
                          <span className="text-xs font-bold text-slate-300">Vanilla Base Game</span>
                          <div className="flex items-center gap-2">
                            <span className="text-[9px] font-mono text-slate-400 px-1.5 py-0.2 rounded bg-slate-950 border border-slate-800">
                              {totalFileLines} lines
                            </span>
                            <button
                              onClick={() => {
                                setEditorContent(currentConflict.base_content);
                                setIsManuallyEdited(false);
                                showToast(`Vanilla base code restored in editor.`);
                              }}
                              className={`px-2 py-0.8 text-[10px] font-semibold rounded border transition cursor-pointer shadow-sm ${
                                editorContent === currentConflict.base_content && !isManuallyEdited
                                  ? 'bg-slate-800 text-emerald-300 border-emerald-700/80 font-bold'
                                  : 'bg-slate-800 text-slate-300 border-slate-700 hover:bg-slate-700 hover:text-white'
                              }`}
                              title="Load original Vanilla base code in the lower editor"
                            >
                              {editorContent === currentConflict.base_content && !isManuallyEdited ? '✓ Base Active' : 'Load Base'}
                            </button>
                          </div>
                        </div>
                        <LinedCodeSnippet
                          content={currentConflict.base_content}
                          vanillaBaseContent={currentConflict.base_content}
                          startLineNum={startLine}
                          targetConflictLine={conflictLine}
                        />
                      </div>

                    {/* Competing Mod Panels */}
                    {currentConflict.competing_mods.map((mod, idx) => {
                      const modUid = mod.mod_id && mod.mod_id !== mod.mod_name
                        ? `${mod.mod_name} (${mod.mod_id})`
                        : `${mod.mod_name}${currentConflict.competing_mods.filter(m => m.mod_name === mod.mod_name).length > 1 ? ` #${idx + 1}` : ''}`;

                      const isModAdded = editorContent.includes(`-- === MOD: [${modUid}] ===`);
                      const isModExclusive = editorContent === mod.content;

                      return (
                        <div
                          key={idx}
                          className={`w-[450px] min-w-[450px] bg-slate-950 border rounded-lg flex flex-col shadow group transition ${
                            isModExclusive
                              ? 'border-emerald-500 ring-1 ring-emerald-500/40'
                              : isModAdded
                              ? 'border-cyan-500/80 ring-1 ring-cyan-500/30'
                              : 'border-slate-800 hover:border-slate-700'
                          }`}
                        >
                          <div className="px-3 py-1.5 bg-slate-900/90 border-b border-slate-800 flex items-center justify-between gap-2">
                            <span className="text-xs font-bold text-emerald-400 truncate max-w-[150px]" title={modUid}>
                              {mod.mod_name}
                            </span>
                            <div className="flex items-center gap-1.5 shrink-0">
                              {/* Toggle Agregar / Quitar Mod */}
                              <button
                                onClick={() => !isCurrentlyPackaged && handleToggleAddMod(mod.content, modUid)}
                                disabled={isCurrentlyPackaged || isModExclusive}
                                className={`px-2 py-1 text-[10px] font-semibold rounded border transition shadow-sm flex items-center gap-1 ${
                                  isCurrentlyPackaged
                                    ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                    : isModExclusive
                                    ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                    : isModAdded
                                    ? 'bg-rose-950/90 text-rose-300 border-rose-700 hover:bg-rose-900 hover:text-white cursor-pointer'
                                    : 'bg-cyan-950/80 text-cyan-300 border-cyan-800 hover:bg-cyan-700 hover:text-white cursor-pointer'
                                }`}
                                title={
                                  isCurrentlyPackaged
                                    ? 'The package is packaged (read-only). Unpack to modify.'
                                    : isModExclusive
                                    ? 'This mod is already loaded exclusively in the editor'
                                    : isModAdded
                                    ? `Remove code block of ${mod.mod_name} from editor`
                                    : `Add code of ${mod.mod_name} to editor`
                                }
                              >
                                {isModAdded ? (
                                  <>
                                    <Minus className="w-3 h-3 text-rose-400" />
                                    <span>Remove Mod</span>
                                  </>
                                ) : (
                                  <>
                                    <Plus className="w-3 h-3 text-cyan-400" />
                                    <span>Add Mod</span>
                                  </>
                                )}
                              </button>

                              {/* Toggle Usar Solo Este Mod / Deshacer */}
                              <button
                                onClick={() => !isCurrentlyPackaged && handleToggleUseSingleMod(mod.content, mod.mod_name)}
                                disabled={isCurrentlyPackaged}
                                className={`px-2 py-1 text-[10px] font-semibold rounded border transition shadow-sm flex items-center gap-1 ${
                                  isCurrentlyPackaged
                                    ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                    : isModExclusive
                                    ? 'bg-amber-950/90 text-amber-300 border-amber-700 hover:bg-amber-900 hover:text-white cursor-pointer'
                                    : 'bg-emerald-950 text-emerald-300 border-emerald-800 hover:bg-emerald-700 hover:text-white cursor-pointer'
                                }`}
                                title={
                                  isCurrentlyPackaged
                                    ? 'The package is packaged (read-only). Unpack to modify.'
                                    : isModExclusive
                                    ? `Undo exclusive selection and return to base code`
                                    : `Replace all editor content with exclusive code of ${mod.mod_name}`
                                }
                              >
                                {isModExclusive ? (
                                  <>
                                    <Undo2 className="w-3 h-3 text-amber-400" />
                                    <span>Remove Only This</span>
                                  </>
                                ) : (
                                  <span>Use Only This Mod</span>
                                )}
                              </button>
                            </div>
                          </div>
                          <LinedCodeSnippet
                            content={mod.content}
                            vanillaBaseContent={currentConflict.base_content}
                            startLineNum={startLine}
                            targetConflictLine={conflictLine}
                          />
                        </div>
                      );
                    })}
                  </div>
                </div>

                {/* Draggable Vertical Splitter Bar */}
                <div
                  onMouseDown={handleMouseDown}
                  className={`h-2.5 bg-slate-900 hover:bg-emerald-600/60 border-y border-slate-800 cursor-ns-resize flex items-center justify-center transition shrink-0 select-none ${
                    isResizing ? 'bg-emerald-500 text-white' : 'text-slate-600 hover:text-white'
                  }`}
                  title="Drag to resize the top panel and editor"
                >
                  <GripHorizontal className="w-5 h-3" />
                </div>

                {/* Bottom Panel: Interactive Monaco Editor */}
                <div
                  style={{ height: `${100 - topHeightPercent}%` }}
                  className="flex flex-col bg-slate-950 p-3 min-h-0 overflow-hidden"
                >
                  <div className="flex items-center justify-between mb-2 shrink-0">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-bold text-emerald-400 flex items-center gap-1.5">
                        <Code2 className="w-4 h-4 text-emerald-400" />
                        <span>Merge Result (Master Patch)</span>
                      </span>
                      {isCurrentlyPackaged ? (
                        <span className="px-2 py-0.5 text-[9px] rounded bg-emerald-950 text-emerald-300 border border-emerald-800/80 font-mono font-bold">
                          🔒 Published (Read Only)
                        </span>
                      ) : (
                        <span className="px-2 py-0.5 text-[9px] rounded bg-amber-950 text-amber-300 border border-amber-800/80 font-mono">
                          {isManuallyEdited ? '✏️ Manually Edited' : 'Draft (Editable)'}
                        </span>
                      )}
                    </div>

                    <div className="flex items-center gap-2">
                      {isCurrentlyPackaged ? (
                        <button
                          onClick={() => setIsUnpackageModalOpen(true)}
                          disabled={isCleaning}
                          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-amber-950 hover:bg-amber-900 text-amber-300 text-xs font-bold border border-amber-700/60 shadow transition cursor-pointer"
                          title="Unpack this package to edit its merged code"
                        >
                          <Unlock className="w-3.5 h-3.5 text-amber-400" />
                          <span>{isCleaning ? 'Opening...' : '🔓 Unmerge / Open to Edit'}</span>
                        </button>
                      ) : (
                        <>
                          <button
                            onClick={handleResetToAutoMerge}
                            disabled={isCurrentConflictAstApplied}
                            className={`flex items-center gap-1 px-3 py-1.5 rounded-lg border text-xs font-semibold transition ${
                              isCurrentConflictAstApplied
                                ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700 cursor-pointer'
                            }`}
                            title={isCurrentConflictAstApplied ? "The automatic AST merge is already applied in this file" : "Restores the automatic merge generated by the AST engine"}
                          >
                            <RotateCcw className={`w-3.5 h-3.5 ${isCurrentConflictAstApplied ? 'text-slate-600' : 'text-cyan-400'}`} />
                            <span>Restore AST Merge</span>
                          </button>

                          <button
                            onClick={handleUnmergeSingleConflict}
                            disabled={isCurrentConflictUnmerged}
                            className={`flex items-center gap-1 px-3 py-1.5 rounded-lg border text-xs font-semibold transition ${
                              isCurrentConflictUnmerged
                                ? 'bg-slate-900 border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700 cursor-pointer'
                            }`}
                            title={isCurrentConflictUnmerged ? "This file is already in its original unmerged state" : "Restores this file to its unmerged base code (removes merged state)"}
                          >
                            <Undo2 className={`w-3.5 h-3.5 ${isCurrentConflictUnmerged ? 'text-slate-600' : 'text-amber-400'}`} />
                            <span>Unmerge this File</span>
                          </button>

                          <button
                            onClick={handleSaveSingleConflict}
                            disabled={isCurrentConflictSaved}
                            className={`flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-bold transition ${
                              isCurrentConflictSaved
                                ? 'bg-slate-900 border border-slate-800 text-slate-600 cursor-not-allowed opacity-50'
                                : 'bg-emerald-600 hover:bg-emerald-500 text-white shadow cursor-pointer'
                            }`}
                            title={isCurrentConflictSaved ? "This file is already saved and resolved (no pending changes)" : "Saves changes to this file and removes Needs Review warning"}
                          >
                            <Check className={`w-3.5 h-3.5 ${isCurrentConflictSaved ? 'text-slate-600' : 'text-white'}`} />
                            <span>{isCurrentConflictSaved ? '✓ Merge Saved' : '✓ Save Merge of this File'}</span>
                          </button>
                        </>
                      )}
                    </div>
                  </div>

                  <div className="flex-1 border border-slate-800 rounded-lg overflow-hidden relative">
                    <Editor
                      height="100%"
                      defaultLanguage={currentConflict.file_type === 'LUA' ? 'lua' : 'plaintext'}
                      theme="vs-dark"
                      value={editorContent}
                      onChange={(val) => {
                        if (!isCurrentlyPackaged) {
                          setEditorContent(val || '');
                          setIsManuallyEdited(true);
                        }
                      }}
                      options={{
                        minimap: { enabled: false },
                        fontSize: 12,
                        fontFamily: 'Consolas, Monaco, monospace',
                        scrollBeyondLastLine: false,
                        smoothScrolling: true,
                        tabSize: 4,
                        wordWrap: 'on',
                        readOnly: isCurrentlyPackaged,
                      }}
                    />
                  </div>
                </div>
              </div>
            ) : (
              <div className="flex-1 flex flex-col items-center justify-center text-slate-500 bg-slate-950/50">
                <GitCompare className="w-12 h-12 mb-2 text-slate-700" />
              </div>
            )}
          </div>
        )}
      </div>
    )}

      {/* Create Package Modal */}
      {isCreateModalOpen && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <form onSubmit={handleCreatePackageSubmit} className="bg-slate-900 border border-slate-800 rounded-2xl p-6 max-w-md w-full space-y-4 shadow-2xl animate-fade-in">
            <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
              <Package className="w-5 h-5 text-emerald-400" />
              <span>Create New Merge Package</span>
            </h3>
            <p className="text-xs text-slate-400 leading-relaxed">
              Enter a name and description for this package. It will be created in DRAFT mode and you can enter to merge its files independently.
            </p>
            <div>
              <label className="block text-[11px] font-bold text-slate-300 mb-1">Package Name:</label>
              <input
                type="text"
                value={newPackageName}
                onChange={(e) => setNewPackageName(e.target.value)}
                placeholder="e.g: MergedPatch2"
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-200 focus:border-emerald-500 focus:outline-none font-mono"
                autoFocus
              />
            </div>
            <div>
              <label className="block text-[11px] font-bold text-slate-300 mb-1">Description (Optional):</label>
              <textarea
                value={newPackageDescription}
                onChange={(e) => setNewPackageDescription(e.target.value)}
                placeholder="e.g: Compatibility merge to resolve collisions between weapon and vehicle mods."
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-200 focus:border-emerald-500 focus:outline-none resize-none h-20"
              />
            </div>
            <div className="flex items-center justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setIsCreateModalOpen(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-bold rounded-lg cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="submit"
                className="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold rounded-lg cursor-pointer"
              >
                Create Package
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Rename Package Modal */}
      {editingPackageFolder && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <form onSubmit={handleRenamePackageSubmit} className="bg-slate-900 border border-slate-800 rounded-2xl p-6 max-w-md w-full space-y-4 shadow-2xl animate-fade-in">
            <h3 className="text-base font-bold text-slate-100 flex items-center gap-2">
              <Edit2 className="w-5 h-5 text-cyan-400" />
              <span>Edit Package</span>
            </h3>
            <div>
              <label className="block text-[11px] font-bold text-slate-300 mb-1">Package Name:</label>
              <input
                type="text"
                value={editingPackageName}
                onChange={(e) => setEditingPackageName(e.target.value)}
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-200 focus:border-cyan-500 focus:outline-none font-mono"
                autoFocus
              />
            </div>
            <div>
              <label className="block text-[11px] font-bold text-slate-300 mb-1">Description (Optional):</label>
              <textarea
                value={editingPackageDescription}
                onChange={(e) => setEditingPackageDescription(e.target.value)}
                placeholder="Mod description..."
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-200 focus:border-cyan-500 focus:outline-none resize-none h-20"
              />
            </div>
            <div className="flex items-center justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setEditingPackageFolder(null)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-bold rounded-lg cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="submit"
                className="px-4 py-2 bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-bold rounded-lg cursor-pointer"
              >
                Save Changes
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Unpackage Confirmation Modal */}
      {isUnpackageModalOpen && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-amber-500/40 rounded-2xl p-6 max-w-md w-full space-y-4 shadow-2xl animate-fade-in text-slate-200">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-xl bg-amber-500/10 text-amber-400 border border-amber-500/30 flex items-center justify-center shrink-0">
                <Unlock className="w-5 h-5 text-amber-400" />
              </div>
              <div>
                <h3 className="text-base font-bold text-slate-100">⚠️ Open Package for Editing?</h3>
                <p className="text-xs text-slate-400 mt-0.5">The package will switch to DRAFT mode.</p>
              </div>
            </div>

            <div className="bg-slate-950 border border-slate-800 rounded-xl p-3.5 space-y-2 text-xs text-slate-300">
              <p>
                By opening the package <b className="text-emerald-400 font-mono">{currentPackage?.display_name}</b>, you will be able to edit its code merges.
              </p>
              <div className="p-2.5 bg-amber-950/40 border border-amber-500/30 rounded-lg text-amber-300 text-[11px] font-mono leading-relaxed">
                ⚠️ <b>Visibility Notice:</b> While this package is in draft, it <b>will NOT be visible in the game Mod List</b> until you click "Package & Publish".
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 pt-2">
              <button
                type="button"
                onClick={() => setIsUnpackageModalOpen(false)}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold rounded-lg cursor-pointer transition"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={async () => {
                  setIsUnpackageModalOpen(false);
                  await handleCleanMasterPatch();
                }}
                className="px-4 py-2 bg-gradient-to-r from-amber-600 to-orange-600 hover:from-amber-500 hover:to-orange-500 text-white text-xs font-bold rounded-lg cursor-pointer shadow transition"
              >
                Confirm and Open Package
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
