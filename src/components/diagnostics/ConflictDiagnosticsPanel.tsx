import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { ModDiagnostic, DiagnosticSeverity } from '../../types';
import { TauriService } from '../../services/tauri';
import type { LucideIcon } from 'lucide-react';
import {
  Stethoscope,
  ScanLine,
  Search,
  X,
  RefreshCw,
  ShieldCheck,
  ShieldAlert,
  Lightbulb,
  FileCode,
  ChevronDown,
  ChevronUp,
  AlertOctagon,
  AlertTriangle,
  Info as InfoIcon,
  Boxes,
  GitBranch,
  FolderX,
  Layers,
  Link2,
  ArrowUpDown,
  Package,
} from 'lucide-react';

interface ConflictDiagnosticsPanelProps {
  /** Absolute path to the user's Zomboid folder. Scans are skipped while empty. */
  userZomboidDir: string;
  /** Highlight the mod row for this id in the parent list (optional). */
  onJumpToMod?: (modId: string) => void;
}

type LoadState = 'idle' | 'loading' | 'ready' | 'error';

/** Ordered for rendering: most severe band first. */
const SEVERITY_ORDER: DiagnosticSeverity[] = ['Error', 'Warning', 'Info'];

interface SeverityStyle {
  label: string;
  /** Icon used for the group header. */
  icon: LucideIcon;
  /** Text accent, e.g. "text-red-400". */
  text: string;
  /** Solid chip, e.g. "bg-red-500/20 text-red-300 border-red-500/50". */
  chip: string;
  /** Left rail + border of an entry card. */
  rail: string;
  /** Background wash of the group header strip. */
  strip: string;
}

/** Reuses the app-wide red / amber / cyan severity convention. */
const SEVERITY_STYLES: Record<DiagnosticSeverity, SeverityStyle> = {
  Error: {
    label: 'Errors',
    icon: ShieldAlert,
    text: 'text-red-400',
    chip: 'bg-red-500/20 text-red-300 border-red-500/50',
    rail: 'border-l-red-500',
    strip: 'bg-red-950/30 border-red-900/40',
  },
  Warning: {
    label: 'Warnings',
    icon: AlertTriangle,
    text: 'text-amber-400',
    chip: 'bg-amber-500/20 text-amber-300 border-amber-500/50',
    rail: 'border-l-amber-500',
    strip: 'bg-amber-950/30 border-amber-900/40',
  },
  Info: {
    label: 'Info',
    icon: InfoIcon,
    text: 'text-cyan-400',
    chip: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50',
    rail: 'border-l-cyan-500',
    strip: 'bg-cyan-950/30 border-cyan-900/40',
  },
};

/**
 * Keys MUST match `ConflictKind` in `src-tauri/src/conflicts/mod.rs` exactly —
 * that enum is `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`, so the wire
 * values are the SCREAMING_SNAKE_CASE form of the Rust variant names.
 * Unknown kinds fall back to Stethoscope rather than crashing.
 */
const KIND_ICONS: Record<string, LucideIcon> = {
  MISSING_DEPENDENCY: FolderX,
  LOAD_ORDER_VIOLATION: ArrowUpDown,
  CIRCULAR_DEPENDENCY: GitBranch,
  INCOMPATIBLE_PAIR: AlertOctagon,
  DUPLICATE_MOD: Boxes,
  GAME_VERSION_MISMATCH: FileCode,
  REQUIRED_LIBRARY_VERSION: Link2,
  MALFORMED_VERSION_DIRECTIVE: AlertTriangle,
  DATA_KEY_COLLISION: Layers,
  ASSET_COLLISION: Package,
  FILE_COLLISION: FileCode,
};

/** Turns DUPLICATE_MOD into "Duplicate Mod" for the readable title line. */
const humanizeKind = (kind: string): string =>
  kind
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(' ');

const matchesQuery = (d: ModDiagnostic, query: string): boolean => {
  if (!query) return true;
  const q = query.toLowerCase();
  return (
    d.mod_ids.some((id) => id.toLowerCase().includes(q)) ||
    d.related_mod_ids.some((id) => id.toLowerCase().includes(q)) ||
    d.title.toLowerCase().includes(q) ||
    d.kind.toLowerCase().includes(q)
  );
};

export const ConflictDiagnosticsPanel: React.FC<ConflictDiagnosticsPanelProps> = ({
  userZomboidDir,
  onJumpToMod,
}) => {
  const [expanded, setExpanded] = useState<boolean>(false);
  const [state, setState] = useState<LoadState>('idle');
  const [diagnostics, setDiagnostics] = useState<ModDiagnostic[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [query, setQuery] = useState<string>('');
  const [mutedSeverities, setMutedSeverities] = useState<DiagnosticSeverity[]>([]);

  const runScan = useCallback(async () => {
    if (!userZomboidDir) {
      setState('error');
      setErrorMessage('No Zomboid user folder is configured. Set your paths in App Settings first.');
      return;
    }

    setState('loading');
    setErrorMessage(null);
    try {
      const results = await TauriService.scanModDiagnostics(userZomboidDir);
      setDiagnostics(results);
      setState('ready');
    } catch (err: any) {
      console.error('Diagnostics scan failed:', err);
      setDiagnostics([]);
      setErrorMessage(err?.message ? String(err.message) : String(err));
      setState('error');
    }
  }, [userZomboidDir]);

  // Scan once, the first time the operator opens the panel.
  useEffect(() => {
    if (expanded && state === 'idle') {
      runScan();
    }
  }, [expanded, state, runScan]);

  // A newly configured workspace should get a fresh scan on next open.
  useEffect(() => {
    if (!userZomboidDir) {
      setState('idle');
      setDiagnostics([]);
      setErrorMessage(null);
    }
  }, [userZomboidDir]);

  const totalBySeverity = useMemo(() => {
    const counts: Record<DiagnosticSeverity, number> = { Error: 0, Warning: 0, Info: 0 };
    for (const d of diagnostics) counts[d.severity] += 1;
    return counts;
  }, [diagnostics]);

  const visibleDiagnostics = useMemo(() => {
    const trimmed = query.trim();
    return diagnostics.filter(
      (d) => !mutedSeverities.includes(d.severity) && matchesQuery(d, trimmed)
    );
  }, [diagnostics, mutedSeverities, query]);

  const grouped = useMemo(() => {
    return SEVERITY_ORDER.map((severity) => ({
      severity,
      items: visibleDiagnostics.filter((d) => d.severity === severity),
    })).filter((g) => g.items.length > 0);
  }, [visibleDiagnostics]);

  const isFiltered = mutedSeverities.length > 0 || query.trim().length > 0;

  const toggleSeverity = (severity: DiagnosticSeverity) => {
    setMutedSeverities((prev) =>
      prev.includes(severity) ? prev.filter((s) => s !== severity) : [...prev, severity]
    );
  };

  const handleJump = (modId: string) => {
    if (onJumpToMod) onJumpToMod(modId);
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
            <Stethoscope className="w-4 h-4 text-cyan-400 shrink-0" />
            <span className="text-[11px] font-bold uppercase tracking-wider text-slate-300">Conflict Diagnostics</span>
            {state === 'ready' && (
              <span className="flex items-center gap-1.5">
                {SEVERITY_ORDER.filter((s) => totalBySeverity[s] > 0).map((s) => {
                  const style = SEVERITY_STYLES[s];
                  return (
                    <span
                      key={s}
                      className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border ${style.chip}`}
                    >
                      {totalBySeverity[s]} {s.toUpperCase()}
                    </span>
                  );
                })}
                {diagnostics.length === 0 && (
                  <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-500/15 text-emerald-300 border-emerald-500/40">
                    CLEAN
                  </span>
                )}
              </span>
            )}
            {state === 'error' && (
              <span className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-red-500/20 text-red-300 border-red-500/50">
                SCAN FAILED
              </span>
            )}
            {state === 'loading' && (
              <RefreshCw className="w-3 h-3 text-cyan-400 animate-spin" />
            )}
          </span>

          <span className="flex items-center gap-1.5 text-[10px] text-slate-500 group-hover:text-slate-300 transition shrink-0">
            <ScanLine className="w-3.5 h-3.5" />
            <span>Deep scan of active mods</span>
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
          <Stethoscope className="w-4 h-4 text-cyan-400 shrink-0" />
          <span className="text-[11px] font-bold uppercase tracking-wider text-slate-200">
            Conflict Diagnostics
          </span>
          {state === 'ready' && (
            <span className="text-[10px] font-mono text-slate-500">
              {diagnostics.length} finding{diagnostics.length === 1 ? '' : 's'}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          <button
            onClick={runScan}
            disabled={state === 'loading'}
            className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-950 hover:bg-slate-800 disabled:opacity-50 text-slate-200 border border-slate-800 hover:border-cyan-700/70 rounded text-[10px] font-bold transition cursor-pointer"
            title="Re-run the cross-mod diagnostic scan"
          >
            <RefreshCw className={`w-3 h-3 text-cyan-400 ${state === 'loading' ? 'animate-spin' : ''}`} />
            <span>{state === 'loading' ? 'Scanning...' : 'Rescan'}</span>
          </button>
          <button
            onClick={() => setExpanded(false)}
            className="p-1 text-slate-500 hover:text-slate-200 transition cursor-pointer"
            title="Collapse diagnostics panel"
          >
            <ChevronUp className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap items-center gap-2 px-3.5 py-2 bg-slate-950/60 border-b border-slate-800">
        <div className="flex items-center gap-1">
          {SEVERITY_ORDER.map((severity) => {
            const style = SEVERITY_STYLES[severity];
            const Icon = style.icon;
            const isMuted = mutedSeverities.includes(severity);
            const count = totalBySeverity[severity];
            return (
              <button
                key={severity}
                onClick={() => toggleSeverity(severity)}
                disabled={count === 0}
                title={isMuted ? `Show ${style.label}` : `Hide ${style.label}`}
                className={`flex items-center gap-1.5 px-2 py-1 rounded border text-[10px] font-mono font-bold transition cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed ${
                  isMuted
                    ? 'bg-slate-900 text-slate-500 border-slate-800 line-through'
                    : `${style.chip} hover:brightness-125`
                }`}
              >
                <Icon className="w-3 h-3 shrink-0" />
                <span>{severity.toUpperCase()}</span>
                <span className="text-[9px] opacity-80">{count}</span>
              </button>
            );
          })}
        </div>

        <div className="relative flex items-center flex-1 min-w-[180px] max-w-xs ml-auto">
          <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter by mod id, title or code..."
            className="w-full bg-slate-950/90 border border-slate-800 hover:border-slate-700 focus:border-cyan-500/80 rounded pl-8 pr-7 py-1 text-[11px] text-slate-200 placeholder-slate-500 transition shadow-inner font-sans outline-none"
          />
          {query && (
            <button
              onClick={() => setQuery('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 p-0.5 cursor-pointer"
              title="Clear filter"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        {isFiltered && (
          <button
            onClick={() => {
              setQuery('');
              setMutedSeverities([]);
            }}
            className="text-[10px] font-mono text-slate-400 hover:text-cyan-300 underline underline-offset-2 cursor-pointer transition"
          >
            Reset
          </button>
        )}
      </div>

      {/* Body */}
      <div className="max-h-[360px] overflow-y-auto divide-y divide-slate-800/70">
        {state === 'loading' && diagnostics.length === 0 && (
          <div className="px-4 py-8 flex flex-col items-center gap-2.5 text-center">
            <RefreshCw className="w-5 h-5 text-cyan-400 animate-spin" />
            <p className="text-[11px] text-slate-400 font-mono">
              Reading mod.info manifests and ModData keys...
            </p>
          </div>
        )}

        {state === 'error' && (
          <div className="p-3.5">
            <div className="bg-red-950/40 border border-red-800/70 rounded-lg p-3 space-y-2.5">
              <div className="flex items-start gap-2">
                <AlertOctagon className="w-4 h-4 text-red-400 shrink-0 mt-px" />
                <div className="min-w-0">
                  <div className="text-xs font-bold text-red-300">Diagnostic scan failed</div>
                  <div className="text-[11px] text-slate-300 leading-relaxed mt-0.5 break-words font-mono">
                    {errorMessage}
                  </div>
                  <div className="flex items-start gap-1.5 mt-2 text-[11px] leading-relaxed">
                    <Lightbulb className="w-3 h-3 text-amber-400 shrink-0 mt-px" />
                    <span className="text-amber-200/90">
                      A scan failure usually means a mod.info could not be read. Close the game if it is
                      running so the Workshop folder is not locked, then retry. If it still fails, build a
                      diagnostic report from <b className="font-mono">Settings</b> and attach it.
                    </span>
                  </div>
                </div>
              </div>
              <div className="flex items-center justify-end pt-0.5">
                <button
                  onClick={runScan}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
                >
                  <RefreshCw className="w-3 h-3" />
                  <span>Retry scan</span>
                </button>
              </div>
            </div>
          </div>
        )}

        {state === 'ready' && grouped.length === 0 && (
          <div className="px-4 py-8 flex flex-col items-center gap-2 text-center">
            {isFiltered ? (
              <>
                <Search className="w-6 h-6 text-slate-600" />
                <p className="text-[11px] text-slate-400">
                  No finding matches the current filter.
                </p>
                <button
                  onClick={() => {
                    setQuery('');
                    setMutedSeverities([]);
                  }}
                  className="text-[10px] font-mono text-cyan-400 hover:text-cyan-300 underline underline-offset-2 cursor-pointer transition"
                >
                  Reset filters
                </button>
              </>
            ) : (
              <>
                <div className="w-11 h-11 rounded-xl bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center">
                  <ShieldCheck className="w-5 h-5 text-emerald-400" />
                </div>
                <p className="text-xs font-bold text-slate-200">No conflicts detected</p>
                <p className="text-[11px] text-slate-400 max-w-md leading-relaxed">
                  Every active mod declares a unique id, resolvable dependencies and no colliding
                  ModData keys.
                </p>
              </>
            )}
          </div>
        )}

        {grouped.map(({ severity, items }) => {
          const style = SEVERITY_STYLES[severity];
          const GroupIcon = style.icon;
          return (
            <div key={severity}>
              {/* Group header */}
              <div
                className={`sticky top-0 z-10 flex items-center justify-between px-3.5 py-1.5 border-b border-slate-800/70 backdrop-blur-sm ${style.strip}`}
              >
                <span className={`flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider ${style.text}`}>
                  <GroupIcon className="w-3.5 h-3.5" />
                  <span>{style.label}</span>
                </span>
                <span className={`text-[10px] font-mono font-bold px-1.5 py-0.5 rounded border ${style.chip}`}>
                  {items.length}
                </span>
              </div>

              {/* Entries */}
              <div className="p-2 space-y-1.5">
                {items.map((d, idx) => {
                  const KindIcon = KIND_ICONS[d.kind] ?? Stethoscope;
                  const allIds = [...d.mod_ids, ...d.related_mod_ids];
                  return (
                    <div
                      key={`${d.kind}-${d.title}-${idx}`}
                      style={{ animationDelay: `${Math.min(idx, 8) * 22}ms` }}
                      className={`animate-panel-rise bg-slate-950/70 hover:bg-slate-950 border border-slate-800 hover:border-slate-700 border-l-4 ${style.rail} rounded-lg px-3 py-2 space-y-1.5 transition`}
                    >
                      {/* Title row */}
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex items-center gap-1.5 min-w-0 flex-wrap">
                          <span
                            className={`flex items-center gap-1 text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border shrink-0 ${style.chip}`}
                            title={humanizeKind(d.kind)}
                          >
                            <KindIcon className="w-2.5 h-2.5" />
                            <span className="uppercase">{d.kind}</span>
                          </span>
                          <span className="text-xs font-bold text-slate-100 truncate">{d.title}</span>
                        </div>
                        <span className={`text-[9px] font-mono font-bold uppercase shrink-0 ${style.text}`}>
                          {d.severity}
                        </span>
                      </div>

                      {/* Plain-language cause */}
                      {d.cause && (
                        <p className="text-[11px] text-slate-300 leading-relaxed">{d.cause}</p>
                      )}

                      {/* Mod ids */}
                      {allIds.length > 0 && (
                        <div className="flex items-center gap-1 flex-wrap pt-0.5">
                          {d.mod_ids.map((id, i) => (
                            <button
                              key={`m-${id}-${i}`}
                              onClick={() => handleJump(id)}
                              disabled={!onJumpToMod}
                              title={onJumpToMod ? `Locate ${id} in the mod list` : id}
                              className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-emerald-950/80 text-emerald-300 border-emerald-700/60 ${
                                onJumpToMod ? 'hover:bg-emerald-950 hover:border-emerald-400 cursor-pointer' : 'cursor-default'
                              } transition`}
                            >
                              {id}
                            </button>
                          ))}

                          {d.related_mod_ids.length > 0 && (
                            <span className="flex items-center gap-1 text-[9px] font-mono text-slate-500">
                              <Link2 className="w-2.5 h-2.5" />
                              <span>related:</span>
                            </span>
                          )}

                          {d.related_mod_ids.map((id, i) => (
                            <button
                              key={`r-${id}-${i}`}
                              onClick={() => handleJump(id)}
                              disabled={!onJumpToMod}
                              title={onJumpToMod ? `Locate ${id} in the mod list` : id}
                              className={`text-[9px] font-mono font-bold px-1.5 py-0.5 rounded border bg-slate-900 text-slate-300 border-slate-700 ${
                                onJumpToMod ? 'hover:bg-slate-800 hover:border-slate-500 cursor-pointer' : 'cursor-default'
                              } transition`}
                            >
                              {id}
                            </button>
                          ))}
                        </div>
                      )}

                      {/* File path */}
                      {d.file_path && (
                        <div
                          className="flex items-center gap-1.5 text-[9.5px] font-mono text-slate-400 min-w-0"
                          title={d.file_path}
                        >
                          <FileCode className="w-3 h-3 text-slate-500 shrink-0" />
                          <span className="truncate">{d.file_path}</span>
                        </div>
                      )}

                      {/* Raw detail */}
                      {d.detail && (
                        <pre className="text-[9.5px] font-mono text-slate-400 bg-slate-900/80 border border-slate-800 rounded px-2 py-1.5 overflow-x-auto whitespace-pre-wrap break-words max-h-24 overflow-y-auto">
                          {d.detail}
                        </pre>
                      )}

                      {/* Suggestion — the actionable half of a finding, so it gets
                          its own highlighted rail rather than a quiet footnote. */}
                      {d.suggestion && (
                        <div className="flex items-start gap-2 pt-1 mt-0.5 border-t border-slate-800/70">
                          <span className="flex items-center gap-1 text-[9px] font-mono font-bold uppercase text-amber-300 bg-amber-500/15 border border-amber-500/50 rounded px-1.5 py-0.5 shrink-0 mt-px">
                            <Lightbulb className="w-2.5 h-2.5" />
                            <span>Fix</span>
                          </span>
                          <span className="text-[11px] text-amber-100 leading-relaxed">{d.suggestion}</span>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default ConflictDiagnosticsPanel;
