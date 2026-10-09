import { TauriService } from './tauri';
import type { ModInfo, ModDiagnostic, LogFileInfoUI } from '../types';

/** Report format marker so a maintainer can tell slices apart. */
export const DIAGNOSTIC_REPORT_VERSION = 1;

/** Cap on the log tail so the report stays pasteable into an issue. */
export const LOG_TAIL_LINE_CAP = 2000;

/** Cap on mod ids listed in the report — full list stays out of the paste. */
const MOD_ID_LIST_CAP = 400;

export interface DiagnosticReportSection {
  title: string;
  lines: string[];
}

export interface DiagnosticReport {
  /** ISO timestamp of when the report was assembled. */
  generatedAt: string;
  /** Redacted, human-readable report body. */
  text: string;
  sections: DiagnosticReportSection[];
  /** Names of the sections that failed to gather, with the reason. */
  failures: { section: string; reason: string }[];
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/**
 * Absolute user paths: `C:\Users\alice\...` and `/home/alice/...`.
 * The Windows profile name is the single most identifying string in a log,
 * so it goes first and is replaced with a stable placeholder.
 */
const USER_PATH_PATTERNS: RegExp[] = [
  /([A-Za-z]:[\\/])Users[\\/]([^\\/\r\n:*?"<>|]+)/g,
  /(\/home\/)([^/\r\n]+)/g,
  /(\/Users\/)([^/\r\n]+)/g,
];

/** `key=value` / `key: value` pairs whose key smells like a credential. */
const SECRET_KEY_PATTERN =
  /(password|passwd|pwd|rcon_password|admin_?password|secret|token|api[_-]?key|apikey|auth|session[_-]?id|bearer|credential)(\s*[:=]\s*)("[^"\r\n]*"|'[^'\r\n]*'|[^\s,;}\])]+)/gi;

/** Long opaque blobs (32+ chars of base64/hex-ish text) are assumed to be keys. */
const OPAQUE_BLOB_PATTERN = /\b[A-Za-z0-9+/_-]{32,}\b/g;

const REDACTED_USER = '<user>';
const REDACTED_SECRET = '<redacted>';

/**
 * Scrubs identifying paths and credential-looking values from arbitrary text.
 * Purely string-based and conservative: it never rewrites structure, only the
 * sensitive substrings, so the report stays readable.
 */
export const redactText = (input: string): string => {
  let out = input;
  for (const pattern of USER_PATH_PATTERNS) {
    out = out.replace(pattern, (_match, prefix: string) => `${prefix}${REDACTED_USER}`);
  }
  out = out.replace(SECRET_KEY_PATTERN, (_match, key: string, sep: string) => `${key}${sep}${REDACTED_SECRET}`);
  out = out.replace(OPAQUE_BLOB_PATTERN, REDACTED_SECRET);
  return out;
};

/** Keeps a path's shape while dropping the profile name. */
export const redactPath = (value: string): string => (value ? redactText(value) : '(not configured)');

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

const safeSection = async (
  title: string,
  failures: { section: string; reason: string }[],
  collect: () => Promise<string[]>
): Promise<DiagnosticReportSection | null> => {
  try {
    return { title, lines: await collect() };
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    console.error(`Diagnostic report section "${title}" failed:`, err);
    failures.push({ section: title, reason: redactText(reason) });
    return null;
  }
};

/** Errors and exceptions in a log tail — the "what actually broke" signal. */
const CRASH_SIGNAL_PATTERN =
  /exception|stack trace|\berror\b|crash|nullpointer|failed|stacktrace|lua error|attempt to (call|index)/i;

const isCrashSignal = (line: string): boolean => CRASH_SIGNAL_PATTERN.test(line);

const pickNewestLog = (logs: LogFileInfoUI[]): LogFileInfoUI | null => {
  if (logs.length === 0) return null;
  return [...logs].sort((a, b) => (b.modified_timestamp || 0) - (a.modified_timestamp || 0))[0];
};

const summariseMods = (mods: ModInfo[]): string[] => {
  const enabled = mods.filter((m) => m.enabled);
  const lines = [
    `installed mods: ${mods.length}`,
    `active mods:    ${enabled.length}`,
    `map mods:       ${mods.filter((m) => m.is_map_mod).length}`,
    `library mods:   ${mods.filter((m) => m.is_library).length}`,
    `studio packages:${' '}${mods.filter((m) => m.mod_id.startsWith('Z_PZModStudio_')).length}`,
  ];

  const listed = mods.slice(0, MOD_ID_LIST_CAP).map((m) => `${m.enabled ? '[x]' : '[ ]'} ${m.mod_id}`);
  if (mods.length > MOD_ID_LIST_CAP) {
    listed.push(`... ${mods.length - MOD_ID_LIST_CAP} more mod ids omitted (cap ${MOD_ID_LIST_CAP})`);
  }
  return lines.concat(['', '--- mods (in load order) ---'], listed);
};

const summariseDiagnostics = (diagnostics: ModDiagnostic[]): string[] => {
  if (diagnostics.length === 0) return ['no findings'];

  const counts = diagnostics.reduce<Record<string, number>>((acc, d) => {
    acc[d.severity] = (acc[d.severity] || 0) + 1;
    return acc;
  }, {});

  const lines = [
    `findings: ${diagnostics.length}`,
    `by severity: Error=${counts.Error || 0} Warning=${counts.Warning || 0} Info=${counts.Info || 0}`,
    '',
  ];

  for (const d of diagnostics) {
    lines.push(`[${d.severity.toUpperCase()}] ${d.kind} — ${d.title}`);
    if (d.cause) lines.push(`  cause:      ${d.cause}`);
    // The backend's `suggestion` is the actionable half of a finding, so it is
    // carried verbatim — this is the text a user is meant to act on.
    if (d.suggestion) lines.push(`  suggestion: ${d.suggestion}`);
    const ids = [...d.mod_ids, ...d.related_mod_ids];
    if (ids.length > 0) lines.push(`  mods:       ${ids.join(', ')}`);
    if (d.file_path) lines.push(`  file:       ${d.file_path}`);
    if (d.detail) lines.push(`  detail:     ${d.detail.replace(/\r?\n/g, ' | ')}`);
    lines.push('');
  }

  return lines;
};

/**
 * Builds one pasteable diagnostic bundle from the commands that already exist in
 * `tauri.ts`. Every section is best-effort: a failing section is recorded in
 * `failures` instead of aborting the whole report.
 */
export const buildDiagnosticReport = async (): Promise<DiagnosticReport> => {
  const failures: { section: string; reason: string }[] = [];
  const sections: DiagnosticReportSection[] = [];

  // Studio paths are needed by almost everything else, so resolve them first
  // with the same call the app already uses on startup.
  type StudioPaths = Awaited<ReturnType<typeof TauriService.getAutoPaths>>;
  let paths: StudioPaths | null = null;
  try {
    paths = await TauriService.getAutoPaths();
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    console.error('Diagnostic report: path detection failed:', err);
    failures.push({ section: 'Studio paths', reason: redactText(reason) });
  }

  const userZomboidDir = paths?.user_zomboid_dir || '';

  const pathsSection = await safeSection('Studio paths', failures, async () => {
    if (!paths) return ['paths unavailable'];
    return [
      `pz_install_dir:     ${redactPath(paths.pz_install_dir)}`,
      `workshop_dir:       ${redactPath(paths.workshop_dir)}`,
      `user_zomboid_dir:   ${redactPath(paths.user_zomboid_dir)}`,
      `mod_list_ini_path:  ${redactPath(paths.mod_list_ini_path)}`,
      `is_valid:           ${paths.is_valid}`,
    ];
  });
  if (pathsSection) sections.push(pathsSection);

  const mods = await safeSection('Installed mods', failures, async () => {
    if (!paths) return ['skipped: studio paths unavailable'];
    return summariseMods(await TauriService.scanAllInstalledMods(paths));
  });
  if (mods) sections.push(mods);

  const diagnostics = await safeSection('Cross-mod diagnostics', failures, async () => {
    if (!userZomboidDir) return ['skipped: no Zomboid user folder configured'];
    return summariseDiagnostics(await TauriService.scanModDiagnostics(userZomboidDir));
  });
  if (diagnostics) sections.push(diagnostics);

  const logSection = await safeSection('Most recent log tail', failures, async () => {
    if (!userZomboidDir) return ['skipped: no Zomboid user folder configured'];
    // Strict readers: the plain wrappers return `[]` on transport failure, which
    // would render as "no log files found" — a false all-clear in the one file
    // a user attaches to a bug report. Here a failure must be visible.
    const logs = await TauriService.listAvailableLogFilesStrict(userZomboidDir);
    const newest = pickNewestLog(logs);
    if (!newest) return ['no log files found under the Zomboid folder'];

    const lines = await TauriService.readLogFileStrict(newest.absolute_path, LOG_TAIL_LINE_CAP);
    // Log lines are the most likely place for absolute user paths, so the tail is
    // redacted as it is read — the raw lines never leave this function.
    const tail = lines.slice(-LOG_TAIL_LINE_CAP).map(redactText);
    const signals = tail.filter(isCrashSignal);

    return [
      `log file:    ${newest.file_name}`,
      `modified:    ${newest.modified_timestamp ? new Date(newest.modified_timestamp * 1000).toISOString() : 'unknown'}`,
      `size:        ${newest.size_bytes} bytes`,
      `lines read:  ${tail.length} (cap ${LOG_TAIL_LINE_CAP}, tail only)`,
      `error lines: ${signals.length}`,
      '',
      '--- error / exception lines ---',
      ...(signals.length > 0 ? signals.slice(-200) : ['(none matched)']),
      '',
      '--- last 120 lines of the log ---',
      ...tail.slice(-120),
    ];
  });
  if (logSection) sections.push(logSection);

  // Final pass: every section line is scrubbed on the way out, so a path that
  // slipped through a section collector cannot reach the exported file.
  const scrubbed = sections.map((s) => ({ title: s.title, lines: s.lines.map(redactText) }));

  const generatedAt = new Date().toISOString();
  const text = renderReportText(generatedAt, scrubbed, failures);
  return { generatedAt, text, sections: scrubbed, failures };
};

const renderReportText = (
  generatedAt: string,
  sections: DiagnosticReportSection[],
  failures: { section: string; reason: string }[]
): string => {
  const header = [
    '========================================================================',
    ' PZ MOD STUDIO — DIAGNOSTIC REPORT',
    ` generated: ${generatedAt}   format: v${DIAGNOSTIC_REPORT_VERSION}`,
    '========================================================================',
    '',
    'REDACTION NOTICE',
    '  Absolute user profile paths (C:\\Users\\<name>, /home/<name>) were replaced',
    '  with <user>. Values whose key looked like a credential (password, token,',
    '  secret, api key, session id) and long opaque blobs were replaced with',
    '  <redacted>. Mod ids, file paths inside the install and diagnostic text are',
    '  otherwise unchanged. Review this file before sharing it publicly.',
    '',
  ].join('\n');

  const body = sections
    .map((s) => ['------------------------------------------------------------------------', `## ${s.title}`, ...s.lines, ''].join('\n'))
    .join('\n');

  const failureBlock =
    failures.length > 0
      ? [
          '------------------------------------------------------------------------',
          '## Sections that could not be collected',
          ...failures.map((f) => `- ${f.section}: ${f.reason}`),
          '',
        ].join('\n')
      : '';

  const footer = ['========================================================================', ''].join('\n');

  return [header, body, failureBlock, footer].filter(Boolean).join('\n');
};

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

export type ReportExportResult =
  | { ok: true; destination: string; method: 'disk' | 'clipboard' }
  | { ok: false; reason: string };

/**
 * Writes the report to disk using `save_server_log_snapshot`, the only existing
 * command in `tauri.ts` that persists arbitrary caller-supplied text. It always
 * targets the Zomboid `Logs/` folder, so the path is chosen by the backend and
 * the picker is not used here.
 *
 * Falls back to the clipboard when no Zomboid folder is configured. There is no
 * generic "write text to an arbitrary path" command in the bridge, so a
 * user-chosen destination is not available without new backend support.
 */
export const exportDiagnosticReportToDisk = async (
  report: DiagnosticReport,
  userZomboidDir: string
): Promise<ReportExportResult> => {
  if (!userZomboidDir) {
    return { ok: false, reason: 'No Zomboid user folder is configured, so there is nowhere to write the report.' };
  }

  try {
    const stamp = report.generatedAt.replace(/[:.]/g, '-');
    const savedPath = await TauriService.saveServerLogSnapshot(
      userZomboidDir,
      `pz-mod-studio-diagnostic-report-${stamp}`,
      report.text.split('\n')
    );
    return { ok: true, destination: savedPath, method: 'disk' };
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    console.error('Failed to write diagnostic report to disk:', err);
    return { ok: false, reason };
  }
};

/** Clipboard escape hatch for when even the Logs folder is unavailable. */
export const copyDiagnosticReportToClipboard = async (report: DiagnosticReport): Promise<ReportExportResult> => {
  try {
    await navigator.clipboard.writeText(report.text);
    return { ok: true, destination: 'clipboard', method: 'clipboard' };
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    return { ok: false, reason };
  }
};

/** Suggested filename, used by the UI copy so users can name it themselves. */
export const diagnosticReportFileName = (generatedAt: string): string =>
  `pz-mod-studio-diagnostic-report-${generatedAt.replace(/[:.]/g, '-')}.txt`;
