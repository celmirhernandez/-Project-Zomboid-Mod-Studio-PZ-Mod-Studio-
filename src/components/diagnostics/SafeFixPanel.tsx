import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { FixPlan, PlannedChange, FixKind, ApplyResult, BackupEntry, FIX_ERROR } from '../../types';
import { TauriService } from '../../services/tauri';
import type { LucideIcon } from 'lucide-react';
import {
  ShieldPlus,
  ShieldCheck,
  ScanLine,
  RefreshCw,
  ChevronDown,
  ChevronUp,
  History,
  Wrench,
  FileCode,
  Lightbulb,
  ArrowUpDown,
  PowerOff,
  Power,
  GitMerge,
  Archive,
  HardDriveDownload,
  AlertOctagon,
  AlertTriangle,
  ShieldQuestion,
  CheckCircle2,
  Info as InfoIcon,
  SkipForward,
  Undo2,
  Trash2,
  Layers,
} from 'lucide-react';

interface SafeFixPanelProps {
  /** Absolute path to the user's Zomboid folder. All IPC is skipped while empty. */
  userZomboidDir: string;
  /** Highlight the mod row for this id in the parent list (optional). */
  onJumpToMod?: (modId: string) => void;
}

type PanelView = 'preview' | 'backups';

/**
 * Preview flow: idle -> loading -> ready -> (applying -> applied | stale | error)
 * Backups flow is independent and lazily loaded on first switch.
 */
type FixState = 'idle' | 'loading' | 'ready' | 'applying' | 'applied' | 'stale' | 'error';

type BackupState = 'idle' | 'loading' | 'ready' | 'error';

interface KindStyle {
  label: string;
  icon: LucideIcon;
  /** Chip colors, reusing the app-wide red / amber / cyan tokens. */
  chip: string;
  rail: string;
}

/**
 * Keys MUST match the Rust `FixKind` variant names — that enum carries no
 * `serde(rename_all)`, so the wire values are verbatim CamelCase variant names.
 * Unknown kinds fall back to Wrench rather than crashing.
 */
const KIND_STYLES: Record<string, KindStyle> = {
  reorder_load_order: {
    label: 'Load order',
    icon: ArrowUpDown,
    chip: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50',
    rail: 'border-l-cyan-500',
  },
  disable_mod: {
    label: 'Disable mod',
    icon: PowerOff,
    chip: 'bg-amber-500/20 text-amber-300 border-amber-500/50',
    rail: 'border-l-amber-500',
  },
  reenable_mod: {
    label: 'Re-enable mod',
    icon: Power,
    chip: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50',
    rail: 'border-l-cyan-500',
  },
  apply_merge: {
    label: 'Apply merge',
    icon: GitMerge,
    chip: 'bg-red-500/20 text-red-300 border-red-500/50',
    rail: 'border-l-red-500',
  },
  patch_file: {
    label: 'Patch file',
    icon: FileCode,
    chip: 'bg-red-500/20 text-red-300 border-red-500/50',
    rail: 'border-l-red-500',
  },
};

const FALLBACK_KIND_STYLE: KindStyle = {
  label: 'Other',
  icon: Wrench,
  chip: 'bg-slate-800 text-slate-300 border-slate-700',
  rail: 'border-l-slate-600',
};

/** Rendering order: writes that touch the disk come last so they read as the scariest. */
const KIND_ORDER: FixKind[] = [
  'reorder_load_order',
  'reenable_mod',
  'disable_mod',
  'apply_merge',
  'patch_file',
];

const kindStyle = (kind: FixKind): KindStyle => KIND_STYLES[kind] ?? FALLBACK_KIND_STYLE;

const formatBytes = (bytes: number): string => {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exp);
  return `${value >= 100 || exp === 0 ? Math.round(value) : value.toFixed(1)} ${units[exp]}`;
};

const formatTimestamp = (unixSeconds: number): string => {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) return 'unknown date';
  const d = new Date(unixSeconds * 1000);
  if (Number.isNaN(d.getTime())) return 'unknown date';
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
};

const lastSegment = (path: string): string => {
  if (!path) return '';
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
};

/**
 * The backend refuses to apply a plan whose described mod state has moved.
 *
 * `apply_mod_fix` has a pinned return type of `ApplyResult` (no `Result`), so a
 * refusal is NOT a rejected command — it arrives as a normal payload whose
 * `errors` array begins with a `FixErrorCode`. Match the typed prefix; the prose
 * fallback is for transport-level rejections that never got a payload at all.
 */
const isStalePlanError = (message: string): boolean =>
  message.includes(FIX_ERROR.planStale) ||
  message.includes(FIX_ERROR.unknownPlan) ||
  /\bstale\b|re-?preview|no longer (valid|current)|plan .*(changed|expired|invalid)|out of date/i.test(message);

/** True when an apply payload carries a refusal rather than applied work. */
const refusalOf = (result: ApplyResult): string | null => {
  const refusal = result.errors.find((e) => e.startsWith(FIX_ERROR.planStale) || e.startsWith(FIX_ERROR.unknownPlan));
  return refusal ?? (result.applied.length === 0 && result.errors.length > 0 ? result.errors[0] : null);
};

/** Fields whose difference makes a re-previewed change materially different. */
const changeFingerprint = (c: PlannedChange): string =>
  `${c.kind}|${c.target_path}|${c.description}|${c.affected_mod_ids.join(',')}|${c.reversible}`;

interface ChangeDelta {
  added: PlannedChange[];
  removed: PlannedChange[];
  modified: { before: PlannedChange; after: PlannedChange }[];
  unchanged: number;
}

/**
 * Compares the refused plan against its fresh replacement so the operator can
 * SEE what moved underneath them instead of being told "try again".
 */
const diffPlans = (before: PlannedChange[], after: PlannedChange[]): ChangeDelta => {
  const beforeById = new Map(before.map((c) => [c.change_id, c]));
  const afterById = new Map(after.map((c) => [c.change_id, c]));

  const added: PlannedChange[] = [];
  const modified: { before: PlannedChange; after: PlannedChange }[] = [];
  let unchanged = 0;

  for (const change of after) {
    const old = beforeById.get(change.change_id);
    if (!old) {
      added.push(change);
    } else if (changeFingerprint(old) !== changeFingerprint(change)) {
      modified.push({ before: old, after: change });
    } else {
      unchanged += 1;
    }
  }

  const removed = before.filter((c) => !afterById.has(c.change_id));
  return { added, removed, modified, unchanged };
};

const ChangeCard: React.FC<{
  change: PlannedChange;
  onJumpToMod?: (modId: string) => void;
  delayIndex?: number;
}> = ({ change, onJumpToMod, delayIndex = 0 }) => {
  const [showDiff, setShowDiff] = useState(false);
  const style = kindStyle(change.kind);
  const KindIcon = style.icon;

  return (
    <div
      style={{ animationDelay: `${Math.min(delayIndex, 8) * 22}ms` }}
      className={`animate-panel-rise bg-slate-950/70 hover:bg-slate-950 border border-slate-800 hover:border-slate-700 border-l-4 ${style.rail} rounded-lg px-3 py-2 space-y-1.5 transition`}
    >
      {/* Header row */}
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
          <span
            className={`flex items-center gap-1 text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border shrink-0 ${style.chip}`}
            title={style.label}
          >
            <KindIcon className="w-2.5 h-2.5" />
            <span className="uppercase">{change.kind}</span>
          </span>
          <span className="text-[9px] font-mono text-slate-500 truncate" title={change.change_id}>
            {change.change_id}
          </span>
        </div>
        <span
          className={`text-[9px] font-mono font-bold uppercase shrink-0 ${
            change.reversible ? 'text-emerald-400' : 'text-red-400'
          }`}
          title={
            change.reversible
              ? 'A backup of the current file is written before this change lands'
              : 'No backup could be reserved for this change — it cannot be rolled back'
          }
        >
          {change.reversible ? 'Reversible' : 'Not reversible'}
        </span>
      </div>

      {/* Plain-language description */}
      {change.description && (
        <p className="text-[11px] text-slate-200 leading-relaxed">{change.description}</p>
      )}

      {/* Affected mods */}
      {change.affected_mod_ids.length > 0 && (
        <div className="flex items-center gap-1 flex-wrap pt-0.5">
          <span className="text-[9px] font-mono text-slate-500">affects:</span>
          {change.affected_mod_ids.map((id, i) => (
            <button
              key={`${id}-${i}`}
              onClick={() => onJumpToMod?.(id)}
              disabled={!onJumpToMod}
              title={onJumpToMod ? `Locate ${id} in the mod list` : id}
              className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-950/80 text-emerald-300 border-emerald-700/60 ${
                onJumpToMod ? 'hover:bg-emerald-950 hover:border-emerald-400 cursor-pointer' : 'cursor-default'
              } transition`}
            >
              {id}
            </button>
          ))}
        </div>
      )}

      {/* Target path */}
      <div className="flex items-center gap-1.5 text-[9.5px] font-mono text-slate-400 min-w-0" title={change.target_path}>
        <FileCode className="w-3 h-3 text-slate-500 shrink-0" />
        <span className="truncate">{change.target_path || '(no path — logical change)'}</span>
      </div>

      {/* Backup destination */}
      {change.backup_path && (
        <div
          className="flex items-center gap-1.5 text-[9.5px] font-mono text-emerald-400/90 min-w-0"
          title={change.backup_path}
        >
          <Archive className="w-3 h-3 text-emerald-500 shrink-0" />
          <span className="truncate">backup → {change.backup_path}</span>
        </div>
      )}

      {/* Read-only diff preview */}
      {change.diff_preview && (
        <div className="pt-0.5">
          <button
            onClick={() => setShowDiff((v) => !v)}
            className="flex items-center gap-1 text-[9px] font-mono font-bold text-slate-400 hover:text-cyan-300 transition cursor-pointer"
          >
            <ChevronDown className={`w-3 h-3 transition-transform ${showDiff ? '' : '-rotate-90'}`} />
            <span>{showDiff ? 'Hide' : 'Show'} diff preview ({change.diff_preview.split('\n').length} lines)</span>
          </button>
          {showDiff && (
            <pre className="mt-1 text-[9.5px] font-mono text-slate-300 bg-slate-950 border border-slate-800 rounded px-2 py-1.5 overflow-auto max-h-48 whitespace-pre leading-relaxed">
              {change.diff_preview}
            </pre>
          )}
        </div>
      )}
    </div>
  );
};

export const SafeFixPanel: React.FC<SafeFixPanelProps> = ({ userZomboidDir, onJumpToMod }) => {
  const [expanded, setExpanded] = useState<boolean>(false);
  const [view, setView] = useState<PanelView>('preview');

  const [fixState, setFixState] = useState<FixState>('idle');
  const [plan, setPlan] = useState<FixPlan | null>(null);
  /** Plan that the backend refused, kept so the re-preview can be diffed against it. */
  const [stalePlan, setStalePlan] = useState<PlannedChange[] | null>(null);
  const [staleMessage, setStaleMessage] = useState<string>('');
  const [changeDelta, setChangeDelta] = useState<ChangeDelta | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [applyResult, setApplyResult] = useState<ApplyResult | null>(null);

  const [backupState, setBackupState] = useState<BackupState>('idle');
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [backupError, setBackupError] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);
  /** Backup id whose destructive confirmation is currently open. */
  const [pendingRestoreId, setPendingRestoreId] = useState<string | null>(null);
  const [restoreOutcome, setRestoreOutcome] = useState<{ id: string; message: string; ok: boolean } | null>(null);

  const missingDir = !userZomboidDir;

  const runPreview = useCallback(async () => {
    if (!userZomboidDir) {
      setFixState('error');
      setErrorMessage('No Zomboid user folder is configured. Set your paths in App Settings first.');
      return;
    }

    setFixState('loading');
    setErrorMessage(null);
    try {
      const next = await TauriService.previewModFixes(userZomboidDir);
      // Diff against the refused plan before swapping it out.
      setChangeDelta((prev) => (stalePlan ? diffPlans(stalePlan, next.changes) : prev));
      setPlan(next);
      setStalePlan(null);
      setStaleMessage('');
      setApplyResult(null);
      setConfirmed(false);
      setFixState('ready');
    } catch (err: any) {
      console.error('Fix preview failed:', err);
      setErrorMessage(err?.message ? String(err.message) : String(err));
      setFixState('error');
    }
  }, [userZomboidDir, stalePlan]);

  const loadBackups = useCallback(async () => {
    if (!userZomboidDir) {
      setBackupState('error');
      setBackupError('No Zomboid user folder is configured. Set your paths in App Settings first.');
      return;
    }

    setBackupState('loading');
    setBackupError(null);
    try {
      const entries = await TauriService.listBackups(userZomboidDir);
      setBackups(entries);
      setBackupState('ready');
    } catch (err: any) {
      console.error('Backup list failed:', err);
      setBackups([]);
      setBackupError(err?.message ? String(err.message) : String(err));
      setBackupState('error');
    }
  }, [userZomboidDir]);

  // Nothing is fetched on mount — the operator opens the panel and decides.
  useEffect(() => {
    if (!userZomboidDir) {
      setFixState('idle');
      setPlan(null);
      setStalePlan(null);
      setChangeDelta(null);
      setApplyResult(null);
      setErrorMessage(null);
      setBackupState('idle');
      setBackups([]);
      setBackupError(null);
    }
  }, [userZomboidDir]);

  // Backups load the first time the operator switches to that view.
  useEffect(() => {
    if (view === 'backups' && backupState === 'idle') {
      loadBackups();
    }
  }, [view, backupState, loadBackups]);

  const groupedChanges = useMemo(() => {
    if (!plan) return [];
    const seen: FixKind[] = [];
    for (const change of plan.changes) {
      if (!seen.includes(change.kind)) seen.push(change.kind);
    }
    // Known kinds first in risk order, unknown kinds trailing.
    const ordered = [
      ...KIND_ORDER.filter((k) => seen.includes(k)),
      ...seen.filter((k) => !KIND_ORDER.includes(k)),
    ];
    return ordered.map((kind) => ({
      kind,
      items: plan.changes.filter((c) => c.kind === kind),
    }));
  }, [plan]);

  const nonReversibleCount = useMemo(
    () => (plan ? plan.changes.filter((c) => !c.reversible).length : 0),
    [plan]
  );

  const handleApply = async () => {
    if (!plan || !confirmed || !userZomboidDir) return;

    setFixState('applying');
    setErrorMessage(null);
    try {
      // The plan id doubles as the deliberate-confirmation token the backend
      // requires alongside `requires_confirmation`.
      const result = await TauriService.applyModFix(userZomboidDir, plan.plan_id, plan.plan_id);
      setConfirmed(false);

      // A refusal comes back as a NORMAL payload (ApplyResult is not a Result),
      // so it must be inspected before we claim the fix landed.
      const refusal = refusalOf(result);
      if (refusal) {
        if (isStalePlanError(refusal)) {
          // Keep the refused plan around so the next preview can be compared to it.
          setStalePlan(plan.changes);
          setStaleMessage(refusal);
          setFixState('stale');
          return;
        }
        setApplyResult(result);
        setErrorMessage(refusal);
        setFixState('error');
        return;
      }

      setApplyResult(result);
      setStalePlan(null);
      setStaleMessage('');
      setFixState('applied');
      // A rollback point now exists — refresh history so it is visible.
      if (view === 'backups') {
        loadBackups();
      } else {
        setBackupState('idle');
      }
    } catch (err: any) {
      const message = err?.message ? String(err.message) : String(err);
      setConfirmed(false);
      if (plan && isStalePlanError(message)) {
        // Keep the refused plan around so the next preview can be compared to it.
        setStalePlan(plan.changes);
        setStaleMessage(message);
        setFixState('stale');
      } else {
        setErrorMessage(message);
        setFixState('error');
      }
    }
  };

  const handleRestore = async (entry: BackupEntry) => {
    if (!userZomboidDir) return;
    setRestoringId(entry.backup_id);
    setRestoreOutcome(null);
    try {
      const result = await TauriService.restoreBackup(userZomboidDir, entry.backup_id);
      const failed = result.errors.length > 0;
      const message = failed
        ? `${result.restored.length} restored, ${result.errors.length} failed: ${result.errors.join('; ')}`
        : `Restored ${result.restored.length} file${result.restored.length === 1 ? '' : 's'} from ${entry.backup_id}.`;
      setRestoreOutcome({ id: entry.backup_id, message, ok: !failed });
      setPendingRestoreId(null);
      loadBackups();
    } catch (err: any) {
      const message = err?.message ? String(err.message) : String(err);
      setRestoreOutcome({ id: entry.backup_id, message, ok: false });
    } finally {
      setRestoringId(null);
    }
  };

  const changeView = (next: PanelView) => {
    setView(next);
    setRestoreOutcome(null);
    setPendingRestoreId(null);
  };

  // ---- Collapsed summary rail -------------------------------------------------
  if (!expanded) {
    return (
      <div className="mb-3 shrink-0">
        <button
          onClick={() => setExpanded(true)}
          className="w-full flex items-center justify-between gap-3 px-3.5 py-2 bg-slate-900/70 hover:bg-slate-900 border border-slate-800 hover:border-slate-700 rounded-lg text-left transition cursor-pointer group"
        >
          <span className="flex items-center gap-2 min-w-0">
            <ShieldPlus className="w-4 h-4 text-emerald-400 shrink-0" />
            <span className="text-[11px] font-bold uppercase tracking-wider text-slate-300">Safe Fix</span>
            {fixState === 'ready' && plan && (
              <span className="flex items-center gap-1.5">
                <span
                  className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border ${
                    plan.changes.length > 0
                      ? 'bg-amber-500/20 text-amber-300 border-amber-500/50'
                      : 'bg-emerald-500/15 text-emerald-300 border-emerald-500/40'
                  }`}
                >
                  {plan.changes.length} CHANGE{plan.changes.length === 1 ? '' : 'S'}
                </span>
                <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-slate-800 text-slate-300 border-slate-700">
                  {plan.affected_files} FILE{plan.affected_files === 1 ? '' : 'S'}
                </span>
              </span>
            )}
            {fixState === 'stale' && (
              <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-amber-500/20 text-amber-300 border-amber-500/50">
                PLAN STALE
              </span>
            )}
            {fixState === 'applied' && (
              <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-500/15 text-emerald-300 border-emerald-500/40">
                APPLIED
              </span>
            )}
            {fixState === 'error' && (
              <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-red-500/20 text-red-300 border-red-500/50">
                FAILED
              </span>
            )}
            {fixState === 'loading' && <RefreshCw className="w-3 h-3 text-cyan-400 animate-spin" />}
          </span>

          <span className="flex items-center gap-1.5 text-[10px] text-slate-500 group-hover:text-slate-300 transition shrink-0">
            <ScanLine className="w-3.5 h-3.5" />
            <span>Preview repairs before anything is written</span>
            <ChevronDown className="w-3.5 h-3.5" />
          </span>
        </button>
      </div>
    );
  }

  // ---- Expanded panel ---------------------------------------------------------
  return (
    <div className="mb-4 shrink-0 bg-slate-900/80 border border-slate-800 rounded-xl shadow overflow-hidden animate-panel-rise">
      {/* Panel header */}
      <div className="flex items-center justify-between gap-3 px-3.5 py-2 bg-slate-900 border-b border-slate-800">
        <div className="flex items-center gap-2 min-w-0">
          <ShieldPlus className="w-4 h-4 text-emerald-400 shrink-0" />
          <span className="text-[11px] font-bold uppercase tracking-wider text-slate-200">Safe Fix</span>
          {view === 'preview' && fixState === 'ready' && plan && (
            <span className="text-[10px] font-mono text-slate-500">
              {plan.changes.length} change{plan.changes.length === 1 ? '' : 's'} · {plan.affected_files} file
              {plan.affected_files === 1 ? '' : 's'}
            </span>
          )}
          {view === 'backups' && backupState === 'ready' && (
            <span className="text-[10px] font-mono text-slate-500">
              {backups.length} backup{backups.length === 1 ? '' : 's'}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          {/* View switch */}
          <div className="flex items-center rounded border border-slate-800 overflow-hidden">
            <button
              onClick={() => changeView('preview')}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold transition cursor-pointer ${
                view === 'preview' ? 'bg-slate-800 text-slate-100' : 'text-slate-500 hover:text-slate-300'
              }`}
              title="Preview proposed repairs"
            >
              <Wrench className="w-3 h-3" />
              <span>Preview</span>
            </button>
            <button
              onClick={() => changeView('backups')}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] font-bold transition cursor-pointer border-l border-slate-800 ${
                view === 'backups' ? 'bg-slate-800 text-slate-100' : 'text-slate-500 hover:text-slate-300'
              }`}
              title="Rollback history"
            >
              <History className="w-3 h-3" />
              <span>Backups</span>
            </button>
          </div>

          {view === 'preview' ? (
            <button
              onClick={runPreview}
              disabled={fixState === 'loading' || fixState === 'applying'}
              className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-950 hover:bg-slate-800 disabled:opacity-50 text-slate-200 border border-slate-800 hover:border-cyan-700/70 rounded text-[10px] font-bold transition cursor-pointer"
              title="Build a fresh repair plan from the current mod state"
            >
              <RefreshCw className={`w-3 h-3 text-cyan-400 ${fixState === 'loading' ? 'animate-spin' : ''}`} />
              <span>{fixState === 'loading' ? 'Planning...' : 'Scan for fixes'}</span>
            </button>
          ) : (
            <button
              onClick={loadBackups}
              disabled={backupState === 'loading'}
              className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-950 hover:bg-slate-800 disabled:opacity-50 text-slate-200 border border-slate-800 hover:border-cyan-700/70 rounded text-[10px] font-bold transition cursor-pointer"
              title="Re-read the backup history from disk"
            >
              <RefreshCw className={`w-3 h-3 text-cyan-400 ${backupState === 'loading' ? 'animate-spin' : ''}`} />
              <span>{backupState === 'loading' ? 'Reading...' : 'Refresh'}</span>
            </button>
          )}

          <button
            onClick={() => setExpanded(false)}
            className="p-1 text-slate-500 hover:text-slate-200 transition cursor-pointer"
            title="Collapse Safe Fix panel"
          >
            <ChevronUp className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* ============================================================ PREVIEW VIEW */}
      {view === 'preview' && (
        <div className="max-h-[420px] overflow-y-auto">
          {/* Idle — the default state. Nothing has been requested from the backend. */}
          {fixState === 'idle' && (
            <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
              <div className="w-11 h-11 rounded-xl bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center">
                <ShieldCheck className="w-5 h-5 text-emerald-400" />
              </div>
              <p className="text-xs font-bold text-slate-200">Nothing has been changed yet</p>
              <p className="text-[11px] text-slate-400 max-w-md leading-relaxed">
                Safe Fix reads your mods and proposes repairs. Nothing is written until you read the whole
                plan and confirm it.
              </p>
              <button
                onClick={runPreview}
                disabled={missingDir}
                className="mt-1 flex items-center gap-1.5 px-3.5 py-1.5 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
              >
                <ScanLine className="w-3.5 h-3.5" />
                <span>Scan for fixes</span>
              </button>
              {missingDir && (
                <p className="text-[10px] font-mono text-amber-400">Configure your Zomboid folder first.</p>
              )}
            </div>
          )}

          {fixState === 'loading' && (
            <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
              <RefreshCw className="w-5 h-5 text-cyan-400 animate-spin" />
              <p className="text-[11px] text-slate-400 font-mono">
                Reading mod state and building a repair plan...
              </p>
            </div>
          )}

          {/* Empty plan: genuinely nothing to do. */}
          {fixState === 'ready' && plan && plan.changes.length === 0 && (
            <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
              <div className="w-11 h-11 rounded-xl bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center">
                <ShieldCheck className="w-5 h-5 text-emerald-400" />
              </div>
              <p className="text-xs font-bold text-slate-200">No fixes needed</p>
              <p className="text-[11px] text-slate-400 max-w-md leading-relaxed">{plan.summary}</p>
              <span className="text-[9px] font-mono text-slate-600">plan {plan.plan_id}</span>
            </div>
          )}

          {/* Ready: the decision surface */}
          {(fixState === 'ready' || fixState === 'applying') && plan && plan.changes.length > 0 && (
            <>
              {/* Stale-plan banner. The whole point: re-preview, then compare. */}
              {fixState === 'ready' && staleMessage && (
                <div className="m-3 bg-amber-950/40 border border-amber-700/70 rounded-lg p-3 space-y-2.5">
                  <div className="flex items-start gap-2">
                    <ShieldQuestion className="w-4 h-4 text-amber-400 shrink-0 mt-px" />
                    <div className="min-w-0 space-y-1">
                      <div className="text-xs font-bold text-amber-200">
                        Your mods changed after this plan was built
                      </div>
                      <p className="text-[11px] text-slate-300 leading-relaxed">
                        The fix was refused because the plan no longer matches what is on disk. Nothing was
                        written. Re-scan to get a plan that reflects the current state, then check what differs
                        before you apply it.
                      </p>
                      <div className="text-[10px] font-mono text-amber-300/80 break-words">{staleMessage}</div>
                    </div>
                  </div>

                  {changeDelta && (
                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
                      <div className="bg-slate-950/70 border border-emerald-800/50 rounded px-2 py-1.5">
                        <div className="text-[9px] font-mono text-emerald-400 uppercase">New</div>
                        <div className="text-xs font-mono font-bold text-emerald-300">{changeDelta.added.length}</div>
                      </div>
                      <div className="bg-slate-950/70 border border-red-800/50 rounded px-2 py-1.5">
                        <div className="text-[9px] font-mono text-red-400 uppercase">Gone</div>
                        <div className="text-xs font-mono font-bold text-red-300">{changeDelta.removed.length}</div>
                      </div>
                      <div className="bg-slate-950/70 border border-amber-800/50 rounded px-2 py-1.5">
                        <div className="text-[9px] font-mono text-amber-400 uppercase">Retargeted</div>
                        <div className="text-xs font-mono font-bold text-amber-300">{changeDelta.modified.length}</div>
                      </div>
                      <div className="bg-slate-950/70 border border-slate-800 rounded px-2 py-1.5">
                        <div className="text-[9px] font-mono text-slate-400 uppercase">Unchanged</div>
                        <div className="text-xs font-mono font-bold text-slate-300">{changeDelta.unchanged}</div>
                      </div>
                    </div>
                  )}

                  {changeDelta &&
                    (changeDelta.added.length > 0 ||
                      changeDelta.removed.length > 0 ||
                      changeDelta.modified.length > 0) && (
                    <details className="bg-slate-950/70 border border-slate-800 rounded px-2.5 py-1.5">
                      <summary className="text-[10px] font-mono text-slate-300 cursor-pointer hover:text-cyan-300 transition">
                        What differs from the refused plan
                      </summary>
                      <div className="mt-2 space-y-2 pt-1">
                        {changeDelta!.added.map((c) => (
                          <div key={`a-${c.change_id}`} className="flex items-start gap-1.5">
                            <span className="text-[9px] font-mono font-bold text-emerald-400 shrink-0 mt-px">NEW</span>
                            <span className="text-[10px] text-slate-300 leading-relaxed">
                              {c.description || c.change_id}
                            </span>
                          </div>
                        ))}
                        {changeDelta!.removed.map((c) => (
                          <div key={`r-${c.change_id}`} className="flex items-start gap-1.5">
                            <span className="text-[9px] font-mono font-bold text-red-400 shrink-0 mt-px">GONE</span>
                            <span className="text-[10px] text-slate-500 leading-relaxed line-through">
                              {c.description || c.change_id}
                            </span>
                          </div>
                        ))}
                        {changeDelta!.modified.map(({ before, after }) => (
                          <div key={`m-${after.change_id}`} className="flex items-start gap-1.5">
                            <span className="text-[9px] font-mono font-bold text-amber-400 shrink-0 mt-px">CHANGED</span>
                            <span className="text-[10px] text-slate-300 leading-relaxed">
                              {before.description || before.change_id}
                              <span className="text-slate-500"> → </span>
                              {after.description || after.change_id}
                            </span>
                          </div>
                        ))}
                      </div>
                    </details>
                  )}

                  <div className="flex items-center justify-end gap-1.5 pt-0.5">
                    <button
                      onClick={runPreview}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                    >
                      <RefreshCw className="w-3 h-3" />
                      <span>Re-scan and show me what changed</span>
                    </button>
                  </div>
                </div>
              )}

              {/* Plan header */}
              <div className="px-3.5 py-2 bg-slate-950/60 border-b border-slate-800 flex items-start justify-between gap-3">
                <div className="min-w-0 space-y-0.5">
                  <div className="text-[11px] font-bold text-slate-200">{plan.summary}</div>
                  <div className="text-[9px] font-mono text-slate-500">plan {plan.plan_id}</div>
                </div>
                <span className="flex items-center gap-1 shrink-0 text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-amber-500/20 text-amber-300 border-amber-500/50">
                  NOT APPLIED
                </span>
              </div>

              {/* Changes grouped by kind */}
              <div className="divide-y divide-slate-800/70">
                {groupedChanges.map(({ kind, items }) => {
                  const style = kindStyle(kind);
                  const GroupIcon = style.icon;
                  return (
                    <div key={kind}>
                      <div
                        className={`sticky top-0 z-10 flex items-center justify-between px-3.5 py-1.5 border-b border-slate-800/70 backdrop-blur-sm ${
                          kind === 'apply_merge' || kind === 'patch_file'
                            ? 'bg-red-950/30 border-red-900/40'
                            : 'bg-cyan-950/30 border-cyan-900/40'
                        }`}
                      >
                        <span
                          className={`flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider ${
                            kind === 'apply_merge' || kind === 'patch_file' ? 'text-red-400' : 'text-cyan-400'
                          }`}
                        >
                          <GroupIcon className="w-3.5 h-3.5" />
                          <span>{style.label}</span>
                        </span>
                        <span className={`text-[10px] font-mono font-bold px-1.5 py-0.5 rounded border ${style.chip}`}>
                          {items.length}
                        </span>
                      </div>

                      <div className="p-2 space-y-1.5">
                        {items.map((c, idx) => (
                          <ChangeCard key={c.change_id} change={c} onJumpToMod={onJumpToMod} delayIndex={idx} />
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Confirmation gate */}
              <div className="sticky bottom-0 bg-slate-900 border-t border-slate-800 px-3.5 py-2.5 space-y-2">
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg px-3 py-2 space-y-1">
                  <div className="text-[10px] font-bold uppercase tracking-wider text-slate-400">
                    Before this runs
                  </div>
                  <ul className="space-y-1">
                    <li className="flex items-start gap-1.5 text-[11px] text-slate-300">
                      <InfoIcon className="w-3 h-3 text-cyan-400 shrink-0 mt-px" />
                      <span>
                        <b className="font-mono text-slate-100">{plan.changes.length}</b> change
                        {plan.changes.length === 1 ? '' : 's'} will be written, touching{' '}
                        <b className="font-mono text-slate-100">{plan.affected_files}</b> file
                        {plan.affected_files === 1 ? '' : 's'} on disk.
                      </span>
                    </li>
                    <li className="flex items-start gap-1.5 text-[11px] text-slate-300">
                      <Archive className="w-3 h-3 text-emerald-400 shrink-0 mt-px" />
                      <span>
                        A backup of each original file is copied first, so the whole run can be rolled back from
                        the Backups tab.
                      </span>
                    </li>
                    {nonReversibleCount > 0 ? (
                      <li className="flex items-start gap-1.5 text-[11px] text-red-300">
                        <AlertTriangle className="w-3 h-3 text-red-400 shrink-0 mt-px" />
                        <span>
                          <b className="font-mono">{nonReversibleCount}</b> of these change
                          {nonReversibleCount === 1 ? '' : 's'} cannot be undone once written.
                        </span>
                      </li>
                    ) : (
                      <li className="flex items-start gap-1.5 text-[11px] text-emerald-300/90">
                        <ShieldCheck className="w-3 h-3 text-emerald-400 shrink-0 mt-px" />
                        <span>Every change in this plan is reversible.</span>
                      </li>
                    )}
                  </ul>
                </div>

                <label className="flex items-start gap-2 cursor-pointer select-none group">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    onChange={(e) => setConfirmed(e.target.checked)}
                    disabled={fixState === 'applying' || !!staleMessage}
                    className="mt-0.5 w-3.5 h-3.5 accent-emerald-500 shrink-0 cursor-pointer disabled:cursor-not-allowed"
                  />
                  <span className="text-[11px] text-slate-300 group-hover:text-slate-200 transition leading-relaxed">
                    I have read the {plan.changes.length} change{plan.changes.length === 1 ? '' : 's'} above and
                    want Safe Fix to write them now.
                    {staleMessage && (
                      <span className="block text-amber-300/90 font-mono text-[10px] mt-0.5">
                        Re-scan before confirming — this plan is out of date.
                      </span>
                    )}
                  </span>
                </label>

                <div className="flex items-center justify-end gap-1.5">
                  <button
                    onClick={() => {
                      setPlan(null);
                      setStalePlan(null);
                      setStaleMessage('');
                      setChangeDelta(null);
                      setApplyResult(null);
                      setErrorMessage(null);
                      setConfirmed(false);
                      setFixState('idle');
                    }}
                    disabled={fixState === 'applying'}
                    className="flex items-center gap-1.5 px-2.5 py-1.5 bg-slate-950 hover:bg-slate-800 disabled:opacity-50 text-slate-300 border border-slate-800 hover:border-slate-600 rounded text-[10px] font-bold transition cursor-pointer"
                    title="Throw this plan away without writing anything"
                  >
                    <Trash2 className="w-3 h-3" />
                    <span>Discard plan</span>
                  </button>
                  <button
                    onClick={handleApply}
                    disabled={!confirmed || fixState === 'applying' || !!staleMessage}
                    className="flex items-center gap-1.5 px-3.5 py-1.5 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                  >
                    {fixState === 'applying' ? (
                      <RefreshCw className="w-3.5 h-3 animate-spin" />
                    ) : (
                      <Wrench className="w-3.5 h-3" />
                    )}
                    <span>{fixState === 'applying' ? 'Applying...' : 'Apply these fixes'}</span>
                  </button>
                </div>
              </div>
            </>
          )}

          {/* Applied result: partial success, skips, errors, and the rollback point */}
          {fixState === 'applied' && applyResult && (
            <div className="p-3 space-y-2.5">
              <div
                className={`bg-emerald-950/40 border rounded-lg p-3 space-y-2 ${
                  applyResult.errors.length > 0 ? 'border-amber-700/70' : 'border-emerald-700/70'
                }`}
              >
                <div className="flex items-start gap-2">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0 mt-px" />
                  <div className="min-w-0 space-y-1">
                    <div className="text-xs font-bold text-emerald-200">
                      {applyResult.applied.length} change{applyResult.applied.length === 1 ? '' : 's'} applied
                      {applyResult.skipped.length > 0 && `, ${applyResult.skipped.length} skipped`}
                    </div>
                    <p className="text-[11px] text-slate-300 leading-relaxed">
                      Every file touched was copied to a backup first, so this run can be undone from the
                      Backups tab.
                    </p>
                  </div>
                </div>

                {applyResult.applied.length > 0 && (
                  <div className="space-y-1">
                    <div className="text-[9px] font-mono text-emerald-400 uppercase tracking-wider">Applied</div>
                    <div className="flex flex-wrap gap-1">
                      {applyResult.applied.map((id) => (
                        <span
                          key={id}
                          className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-950/80 text-emerald-300 border-emerald-700/60"
                          title={id}
                        >
                          {id}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              {applyResult.skipped.length > 0 && (
                <div className="bg-amber-950/30 border border-amber-800/60 rounded-lg p-3 space-y-1.5">
                  <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-amber-300">
                    <SkipForward className="w-3.5 h-3.5" />
                    <span>Skipped ({applyResult.skipped.length})</span>
                  </div>
                  {applyResult.skipped.map((s) => (
                    <div key={s.change_id} className="flex items-start gap-2">
                      <span className="text-[9px] font-mono font-bold text-amber-300 shrink-0 mt-px">{s.change_id}</span>
                      <span className="text-[11px] text-slate-300 leading-relaxed">{s.reason}</span>
                    </div>
                  ))}
                </div>
              )}

              {applyResult.errors.length > 0 && (
                <div className="bg-red-950/40 border border-red-800/70 rounded-lg p-3 space-y-1.5">
                  <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-red-300">
                    <AlertOctagon className="w-3.5 h-3.5" />
                    <span>Errors ({applyResult.errors.length})</span>
                  </div>
                  {applyResult.errors.map((e, i) => (
                    <div key={`err-${i}`} className="text-[11px] font-mono text-red-200 break-words">
                      {e}
                    </div>
                  ))}
                </div>
              )}

              {applyResult.backups.length > 0 && (
                <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-3 space-y-1.5">
                  <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-slate-300">
                    <Archive className="w-3.5 h-3.5 text-emerald-400" />
                    <span>Rollback points created</span>
                  </div>
                  {applyResult.backups.map((b) => (
                    <div key={b.backup_id} className="flex items-center gap-2 min-w-0">
                      <span className="text-[9px] font-mono text-emerald-300 shrink-0" title={b.backup_id}>
                        {b.backup_id}
                      </span>
                      <span className="text-[9.5px] font-mono text-slate-400 truncate" title={b.original_path}>
                        {b.original_path}
                      </span>
                      <span className="text-[9px] font-mono text-slate-600 shrink-0">
                        {formatBytes(b.size_bytes)}
                      </span>
                    </div>
                  ))}
                  <div className="pt-0.5">
                    <button
                      onClick={() => changeView('backups')}
                      className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded text-[10px] font-bold transition cursor-pointer"
                    >
                      <Undo2 className="w-3 h-3" />
                      <span>Open rollback history</span>
                    </button>
                  </div>
                </div>
              )}

              <div className="flex items-center justify-end">
                <button
                  onClick={runPreview}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-cyan-700 hover:bg-cyan-600 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                >
                  <ScanLine className="w-3 h-3" />
                  <span>Scan again</span>
                </button>
              </div>
            </div>
          )}

          {/* Hard error (preview or apply, not stale) */}
          {fixState === 'error' && (
            <div className="p-3.5">
              <div className="bg-red-950/40 border border-red-800/70 rounded-lg p-3 space-y-2.5">
                <div className="flex items-start gap-2">
                  <AlertOctagon className="w-4 h-4 text-red-400 shrink-0 mt-px" />
                  <div className="min-w-0">
                    <div className="text-xs font-bold text-red-300">Safe Fix could not continue</div>
                    <div className="text-[11px] text-slate-300 leading-relaxed mt-0.5 break-words font-mono">
                      {errorMessage}
                    </div>
                    <div className="flex items-start gap-1.5 mt-2 text-[11px] leading-relaxed">
                      <Lightbulb className="w-3 h-3 text-amber-400 shrink-0 mt-px" />
                      <span className="text-amber-200/90">
                        Nothing was written. Re-scan to rebuild the plan from the current mod state; if the
                        scan itself keeps failing, open <b className="font-mono">Conflict Diagnostics</b>{' '}
                        above — a missing dependency or duplicate id there is usually the real cause.
                      </span>
                    </div>
                  </div>
                </div>
                <div className="flex items-center justify-end gap-1.5 pt-0.5">
                  <button
                    onClick={() => {
                      setFixState('idle');
                      setErrorMessage(null);
                    }}
                    className="flex items-center gap-1.5 px-2.5 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded text-[10px] font-bold transition cursor-pointer"
                  >
                    <span>Dismiss</span>
                  </button>
                  <button
                    onClick={runPreview}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                  >
                    <RefreshCw className="w-3 h-3" />
                    <span>Try again</span>
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* ============================================================ BACKUPS VIEW */}
      {view === 'backups' && (
        <div className="max-h-[420px] overflow-y-auto">
          <div className="px-3.5 py-2 bg-slate-950/60 border-b border-slate-800 flex items-center gap-2">
            <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
            <p className="text-[11px] text-slate-300 leading-relaxed">
              Restoring a backup <b className="text-amber-300">overwrites the current file on disk</b> with the
              copy taken before the fix ran. Your present-day file is not kept.
            </p>
          </div>

          <div className="divide-y divide-slate-800/70">
            {backupState === 'loading' && backups.length === 0 && (
              <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
                <RefreshCw className="w-5 h-5 text-cyan-400 animate-spin" />
                <p className="text-[11px] text-slate-400 font-mono">Reading backup history...</p>
              </div>
            )}

            {backupState === 'error' && (
              <div className="p-3.5">
                <div className="bg-red-950/40 border border-red-800/70 rounded-lg p-3 space-y-2.5">
                  <div className="flex items-start gap-2">
                    <AlertOctagon className="w-4 h-4 text-red-400 shrink-0 mt-px" />
                    <div className="min-w-0">
                      <div className="text-xs font-bold text-red-300">Could not read backup history</div>
                      <div className="text-[11px] text-slate-300 leading-relaxed mt-0.5 break-words font-mono">
                        {backupError}
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center justify-end pt-0.5">
                    <button
                      onClick={loadBackups}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                    >
                      <RefreshCw className="w-3 h-3" />
                      <span>Retry</span>
                    </button>
                  </div>
                </div>
              </div>
            )}

            {backupState === 'ready' && backups.length === 0 && (
              <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
                <div className="w-11 h-11 rounded-xl bg-slate-800/60 border border-slate-700 flex items-center justify-center">
                  <Archive className="w-5 h-5 text-slate-500" />
                </div>
                <p className="text-xs font-bold text-slate-200">No backups yet</p>
                <p className="text-[11px] text-slate-400 max-w-md leading-relaxed">
                  Every Safe Fix run copies the files it is about to touch. Once you apply one, its rollback
                  point shows up here.
                </p>
              </div>
            )}

            {backups.map((b, idx) => {
              const isPending = pendingRestoreId === b.backup_id;
              const isBusy = restoringId === b.backup_id;
              const outcome = restoreOutcome?.id === b.backup_id ? restoreOutcome : null;
              return (
                <div
                  key={b.backup_id}
                  style={{ animationDelay: `${Math.min(idx, 8) * 22}ms` }}
                  className="animate-panel-rise bg-slate-950/60 hover:bg-slate-950 border-l-4 border-l-emerald-600 px-3.5 py-2.5 space-y-1.5 transition"
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
                      <span
                        className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-950/80 text-emerald-300 border-emerald-700/60 shrink-0"
                        title={b.backup_id}
                      >
                        {b.backup_id}
                      </span>
                      <span className="text-[9px] font-mono text-slate-500 uppercase">{b.source || 'unknown source'}</span>
                    </div>
                    <span className="text-[9px] font-mono font-bold text-slate-400 shrink-0">
                      {formatBytes(b.size_bytes)}
                    </span>
                  </div>

                  <div className="flex items-center gap-1.5 text-[10px] font-mono text-slate-300 min-w-0">
                    <HardDriveDownload className="w-3 h-3 text-slate-500 shrink-0" />
                    <span className="truncate" title={b.original_path}>
                      {b.original_path}
                    </span>
                  </div>

                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[9px] font-mono text-slate-500">
                      {formatTimestamp(b.created_at_unix)}
                      {b.backup_path && (
                        <span className="ml-2 text-slate-600" title={b.backup_path}>
                          · {lastSegment(b.backup_path)}
                        </span>
                      )}
                    </span>

                    {!isPending && (
                      <button
                        onClick={() => {
                          setPendingRestoreId(b.backup_id);
                          setRestoreOutcome(null);
                        }}
                        className="flex items-center gap-1 px-2 py-1 bg-red-950/60 hover:bg-red-900/70 text-red-300 border border-red-800/70 rounded text-[9px] font-mono font-bold transition cursor-pointer shrink-0"
                        title="Overwrite the current file with this backup"
                      >
                        <Undo2 className="w-3 h-3" />
                        <span>Restore</span>
                      </button>
                    )}
                  </div>

                  {/* Destructive confirmation — must be explicit and per-entry. */}
                  {isPending && (
                    <div className="bg-red-950/50 border border-red-700/70 rounded-lg px-2.5 py-2 space-y-2">
                      <div className="flex items-start gap-1.5">
                        <AlertOctagon className="w-3.5 h-3.5 text-red-400 shrink-0 mt-px" />
                        <p className="text-[10px] text-red-200 leading-relaxed">
                          This overwrites{' '}
                          <span className="font-mono break-all">{b.original_path || 'the original file'}</span>{' '}
                          with the copy from {formatTimestamp(b.created_at_unix)}. Anything written since then is
                          lost.
                        </p>
                      </div>
                      <div className="flex items-center justify-end gap-1.5">
                        <button
                          onClick={() => setPendingRestoreId(null)}
                          disabled={isBusy}
                          className="px-2 py-1 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 rounded text-[9px] font-mono font-bold transition cursor-pointer"
                        >
                          Keep current file
                        </button>
                        <button
                          onClick={() => handleRestore(b)}
                          disabled={isBusy}
                          className="flex items-center gap-1 px-2.5 py-1 bg-red-600 hover:bg-red-500 disabled:opacity-50 text-white rounded text-[9px] font-mono font-bold transition cursor-pointer shadow"
                        >
                          {isBusy ? <RefreshCw className="w-3 h-3 animate-spin" /> : <Layers className="w-3 h-3" />}
                          <span>{isBusy ? 'Restoring...' : 'Yes, overwrite'}</span>
                        </button>
                      </div>
                    </div>
                  )}

                  {outcome && (
                    <div
                      className={`text-[10px] font-mono leading-relaxed break-words px-2 py-1 rounded border ${
                        outcome.ok
                          ? 'bg-emerald-950/40 text-emerald-300 border-emerald-800/60'
                          : 'bg-red-950/40 text-red-300 border-red-800/60'
                      }`}
                    >
                      {outcome.message}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
};

export default SafeFixPanel;