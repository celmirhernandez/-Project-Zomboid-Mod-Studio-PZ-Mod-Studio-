import React from 'react';
import { AlertOctagon, X } from 'lucide-react';

interface InlineErrorProps {
  /** Full message shown to the user. Keep the backend text verbatim. */
  message: string | null;
  /** Optional short headline above the message. */
  title?: string;
  /** Optional retry affordance; hidden when omitted. */
  onRetry?: () => void;
  /** Optional label for the retry button. */
  retryLabel?: string;
  onDismiss?: () => void;
  className?: string;
}

/**
 * Dense inline error strip used in place of the raw `alert()` calls that used to
 * dump backend error strings at users. Renders nothing when there is no message,
 * so callers can render it unconditionally next to their existing status banner.
 *
 * Visual language matches the diagnostics panels: slate-950 rail, red border,
 * 11px body text, monospace message.
 */
export const InlineError: React.FC<InlineErrorProps> = ({
  message,
  title = 'Something went wrong',
  onRetry,
  retryLabel = 'Retry',
  onDismiss,
  className = '',
}) => {
  if (!message) return null;

  return (
    <div
      role="alert"
      className={`bg-red-950/40 border border-red-800/70 border-l-4 border-l-red-500 rounded-lg px-3.5 py-2.5 space-y-2 animate-fade-in ${className}`}
    >
      <div className="flex items-start gap-2">
        <AlertOctagon className="w-4 h-4 text-red-400 shrink-0 mt-px" />
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-bold uppercase tracking-wider text-red-300">{title}</div>
          <div className="text-[11px] text-slate-200 leading-relaxed mt-0.5 break-words font-mono">
            {message}
          </div>
        </div>
        {onDismiss && (
          <button
            onClick={onDismiss}
            className="p-0.5 text-slate-500 hover:text-slate-200 transition cursor-pointer shrink-0"
            title="Dismiss this error"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {onRetry && (
        <div className="flex items-center justify-end pt-0.5">
          <button
            onClick={onRetry}
            className="flex items-center gap-1.5 px-2.5 py-1 bg-red-600 hover:bg-red-500 text-white rounded text-[10px] font-bold transition cursor-pointer shadow"
          >
            <span>{retryLabel}</span>
          </button>
        </div>
      )}
    </div>
  );
};

export default InlineError;
