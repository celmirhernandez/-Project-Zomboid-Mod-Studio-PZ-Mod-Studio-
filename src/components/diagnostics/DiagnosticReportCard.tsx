import React, { useState } from 'react';
import {
  FileWarning,
  Copy,
  Check,
  RefreshCw,
  Download,
  ShieldCheck,
  AlertTriangle,
} from 'lucide-react';
import {
  buildDiagnosticReport,
  exportDiagnosticReportToDisk,
  copyDiagnosticReportToClipboard,
  diagnosticReportFileName,
  type DiagnosticReport,
} from '../../services/diagnosticReport';

interface DiagnosticReportCardProps {
  /** Absolute path to the Zomboid user folder; the disk export needs it. */
  userZomboidDir: string;
}

type BuildState = 'idle' | 'building' | 'ready';

/**
 * One-click bundle for bug reports: studio paths, mod summary, cross-mod
 * diagnostics and the tail of the newest log, redacted and written to disk.
 */
export const DiagnosticReportCard: React.FC<DiagnosticReportCardProps> = ({ userZomboidDir }) => {
  const [state, setState] = useState<BuildState>('idle');
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const handleBuild = async () => {
    setState('building');
    setErrorMessage(null);
    setDestination(null);
    try {
      const built = await buildDiagnosticReport();
      setReport(built);
      setState('ready');
    } catch (err: any) {
      setErrorMessage(err?.message ? String(err.message) : String(err));
      setState('idle');
    }
  };

  const handleSave = async () => {
    if (!report) return;
    setErrorMessage(null);
    const result = await exportDiagnosticReportToDisk(report, userZomboidDir);
    if (result.ok) {
      setDestination(result.destination);
    } else {
      setErrorMessage(result.reason);
    }
  };

  const handleCopy = async () => {
    if (!report) return;
    setErrorMessage(null);
    const result = await copyDiagnosticReportToClipboard(report);
    if (result.ok) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2500);
    } else {
      setErrorMessage(result.reason);
    }
  };

  return (
    <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-4 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <label className="text-xs font-bold text-slate-200 flex items-center gap-1.5">
          <FileWarning className="w-4 h-4 text-cyan-400" />
          Diagnostic Report
        </label>
        <span className="text-[10px] font-bold font-mono px-2 py-0.5 rounded bg-slate-950 text-slate-400 border border-slate-800 shrink-0">
          FOR BUG REPORTS
        </span>
      </div>

      <p className="text-[11px] text-slate-400 leading-relaxed">
        Collects your studio paths, installed mod summary, cross-mod diagnostics and the tail of your newest
        log into a single text file. Absolute user paths and credential-looking values are replaced with{' '}
        <span className="font-mono text-slate-300">&lt;user&gt;</span> /{' '}
        <span className="font-mono text-slate-300">&lt;redacted&gt;</span> before anything is written.
      </p>

      <div className="flex items-center gap-2 flex-wrap">
        <button
          onClick={handleBuild}
          disabled={state === 'building'}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-cyan-700 hover:bg-cyan-600 disabled:opacity-50 text-white rounded-lg text-xs font-bold transition cursor-pointer shadow"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${state === 'building' ? 'animate-spin' : ''}`} />
          <span>{state === 'building' ? 'Collecting...' : report ? 'Rebuild report' : 'Build diagnostic report'}</span>
        </button>

        {report && (
          <>
            <button
              onClick={handleSave}
              disabled={!userZomboidDir}
              title={
                userZomboidDir
                  ? 'Write the report into your Zomboid Logs folder'
                  : 'Configure your Zomboid folder first'
              }
              className="flex items-center gap-1.5 px-3 py-1.5 bg-emerald-700 hover:bg-emerald-600 disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-lg text-xs font-bold transition cursor-pointer shadow"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Save to Logs folder</span>
            </button>
            <button
              onClick={handleCopy}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded-lg text-xs font-bold transition cursor-pointer"
            >
              {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5 text-cyan-400" />}
              <span>{copied ? 'Copied' : 'Copy to clipboard'}</span>
            </button>
          </>
        )}
      </div>

      {report && (
        <div className="bg-slate-950/70 border border-slate-800 rounded-lg p-3 space-y-1.5">
          <div className="flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-wider text-emerald-400">
            <ShieldCheck className="w-3.5 h-3.5" />
            <span>Redacted and ready</span>
          </div>
          <div className="text-[10px] font-mono text-slate-400">
            {report.sections.length} section{report.sections.length === 1 ? '' : 's'} ·{' '}
            {report.text.split('\n').length} lines
          </div>
          <div className="text-[10px] font-mono text-slate-500 break-all">
            suggested name: {diagnosticReportFileName(report.generatedAt)}
          </div>
          {report.failures.length > 0 && (
            <div className="flex items-start gap-1.5 pt-1 text-[10px] text-amber-300">
              <AlertTriangle className="w-3 h-3 text-amber-400 shrink-0 mt-px" />
              <span>
                {report.failures.length} section{report.failures.length === 1 ? '' : 's'} could not be collected:{' '}
                {report.failures.map((f) => f.section).join(', ')}
              </span>
            </div>
          )}
        </div>
      )}

      {destination && (
        <div className="bg-emerald-950/40 border border-emerald-800/60 rounded-lg px-3 py-2 space-y-0.5">
          <div className="text-[10px] font-bold uppercase tracking-wider text-emerald-300">Report written to</div>
          <div className="text-[10px] font-mono text-slate-300 break-all select-text">{destination}</div>
        </div>
      )}

      {errorMessage && (
        <div role="alert" className="bg-red-950/40 border border-red-800/70 border-l-4 border-l-red-500 rounded-lg px-3 py-2 space-y-1">
          <div className="flex items-start gap-2">
            <AlertTriangle className="w-3.5 h-3.5 text-red-400 shrink-0 mt-px" />
            <div className="min-w-0">
              <div className="text-[11px] font-bold uppercase tracking-wider text-red-300">
                Report export problem
              </div>
              <div className="text-[11px] text-slate-200 leading-relaxed break-words font-mono">
                {errorMessage}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default DiagnosticReportCard;
