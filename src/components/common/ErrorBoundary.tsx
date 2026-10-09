import React from 'react';
import { AlertOctagon, RefreshCw, ChevronDown, ChevronUp, Copy, Check } from 'lucide-react';

interface ErrorBoundaryProps {
  /** Human-readable name of the module/panel this boundary guards. */
  name: string;
  /** Optional extra recovery action rendered next to "Reload". */
  onReset?: () => void;
  /** Called once per caught error so callers can log it out of band. */
  onError?: (error: Error, name: string) => void;
  children: React.ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
  componentStack: string;
  showDetails: boolean;
  copied: boolean;
}

/**
 * Top-level safety net for a studio module.
 *
 * React 19 still requires a class component for `componentDidCatch`, so this is
 * deliberately not a hook. It NEVER retries on its own: an automatic re-render
 * loop would hide the bug and burn CPU, so recovery is always an explicit user
 * action (Reload / Try again).
 */
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null, componentStack: '', showDetails: false, copied: false };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error, showDetails: false, copied: false };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    // Keep the stack in state so the "technical details" disclosure can show it.
    this.setState({ componentStack: info.componentStack || '' });
    console.error(`[ErrorBoundary] ${this.props.name} crashed:`, error, info.componentStack);
    try {
      this.props.onError?.(error, this.props.name);
    } catch {
      // A failing logger must never re-trigger the boundary.
    }
  }

  private handleReload = (): void => {
    this.setState({ error: null, componentStack: '', showDetails: false, copied: false });
    try {
      this.props.onReset?.();
    } catch {
      // Ignore — the reload below still runs.
    }
    window.location.reload();
  };

  private handleRetry = (): void => {
    // Clears the error without a full page reload: lets the parent re-fetch and
    // re-render the module in place.
    this.setState({ error: null, componentStack: '', showDetails: false, copied: false });
    try {
      this.props.onReset?.();
    } catch (err) {
      console.error(`[ErrorBoundary] reset handler for ${this.props.name} failed:`, err);
    }
  };

  private handleCopyDetails = async (): Promise<void> => {
    const { error, componentStack } = this.state;
    if (!error) return;
    const text = [
      `PZ Mod Studio — ${this.props.name}`,
      `Time: ${new Date().toISOString()}`,
      '',
      error.message,
      '',
      error.stack || '(no stack)',
      '',
      componentStack || '(no component stack)',
    ].join('\n');
    try {
      await navigator.clipboard.writeText(text);
      this.setState({ copied: true });
      window.setTimeout(() => this.setState({ copied: false }), 2000);
    } catch (err) {
      console.error('Failed to copy error details:', err);
    }
  };

  render(): React.ReactNode {
    const { error, componentStack, showDetails, copied } = this.state;

    if (!error) {
      return this.props.children;
    }

    const details = [
      error.message,
      '',
      error.stack || '(no stack available)',
      '',
      componentStack || '(no component stack available)',
    ].join('\n');

    return (
      <div className="w-full h-full min-h-0 flex items-center justify-center p-6 bg-slate-950 text-slate-100 font-sans overflow-y-auto">
        <div className="w-full max-w-xl bg-slate-900/80 border border-slate-800 rounded-xl shadow overflow-hidden animate-panel-rise">
          {/* Header strip */}
          <div className="flex items-center gap-2.5 px-4 py-2.5 bg-slate-900 border-b border-slate-800">
            <div className="w-8 h-8 rounded-lg bg-red-500/10 border border-red-500/30 flex items-center justify-center shrink-0">
              <AlertOctagon className="w-4 h-4 text-red-400" />
            </div>
            <div className="min-w-0">
              <div className="text-xs font-bold uppercase tracking-wider text-slate-200 truncate">
                {this.props.name} stopped working
              </div>
              <div className="text-[10px] font-mono text-slate-500">Unrecoverable error — the rest of the studio is still open</div>
            </div>
          </div>

          {/* Calm body: plain language first, never a raw trace as the headline */}
          <div className="p-4 space-y-3">
            <p className="text-[11px] text-slate-300 leading-relaxed">
              Something in this module ran into an error it could not recover from. Your work on disk —
              mods, load order and merge packages — is untouched.
            </p>

            <div className="flex items-start gap-1.5">
              <span className="text-[9px] font-mono font-bold uppercase text-amber-400 border border-amber-500/40 bg-amber-500/15 rounded px-1.5 py-0.5 shrink-0 mt-px">
                What to do
              </span>
              <span className="text-[11px] text-slate-300 leading-relaxed">
                Try the operation again. If it keeps failing, use{' '}
                <span className="font-mono text-cyan-300">Build diagnostic report</span> in Settings and attach it
                to your bug report.
              </span>
            </div>

            {/* Collapsible technical details for bug reports */}
            <div className="bg-slate-950/70 border border-slate-800 rounded-lg">
              <button
                onClick={() => this.setState((s) => ({ showDetails: !s.showDetails }))}
                className="w-full flex items-center justify-between gap-2 px-3 py-1.5 text-left transition cursor-pointer"
                title="Show the raw error for bug reports"
              >
                <span className="text-[10px] font-mono font-bold uppercase tracking-wider text-slate-400">
                  Technical details
                </span>
                {showDetails ? (
                  <ChevronUp className="w-3.5 h-3.5 text-slate-500 shrink-0" />
                ) : (
                  <ChevronDown className="w-3.5 h-3.5 text-slate-500 shrink-0" />
                )}
              </button>

              {showDetails && (
                <div className="px-3 pb-2.5 pt-0.5 space-y-2">
                  <pre className="text-[9.5px] font-mono text-slate-400 bg-slate-900/80 border border-slate-800 rounded px-2 py-1.5 overflow-auto max-h-56 whitespace-pre-wrap break-words leading-relaxed select-text">
                    {details}
                  </pre>
                  <div className="flex items-center justify-end">
                    <button
                      onClick={this.handleCopyDetails}
                      className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded text-[10px] font-bold transition cursor-pointer"
                    >
                      {copied ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3 text-cyan-400" />}
                      <span>{copied ? 'Copied' : 'Copy details'}</span>
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Recovery actions */}
          <div className="flex items-center justify-end gap-2 px-4 py-2.5 bg-slate-950/60 border-t border-slate-800">
            <button
              onClick={this.handleRetry}
              className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded text-[10px] font-bold transition cursor-pointer"
            >
              Dismiss and continue
            </button>
            <button
              onClick={this.handleReload}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
            >
              <RefreshCw className="w-3 h-3" />
              <span>Reload studio</span>
            </button>
          </div>
        </div>
      </div>
    );
  }
}

export default ErrorBoundary;
