import { useEffect, useState } from "react";
import { CheckCircle2, ExternalLink, MessageSquarePlus, Send, X } from "lucide-react";
import { getTranslations } from "../lib/i18n";
import { submitFeedback } from "../lib/tauri";
import type { FeedbackCategory, SubmitFeedbackResponse, UiLanguage } from "../types/mangora";

interface FeedbackModalProps {
  open: boolean;
  language: UiLanguage;
  onClose: () => void;
  onOpenExternal: (url: string) => Promise<void>;
  onNotify: (msg: string) => void;
}

export function FeedbackModal({
  open,
  language,
  onClose,
  onOpenExternal,
  onNotify,
}: FeedbackModalProps) {
  const t = getTranslations(language).feedback;
  const [category, setCategory] = useState<FeedbackCategory>("suggestion");
  const [message, setMessage] = useState("");
  const [includeDiagnostics, setIncludeDiagnostics] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SubmitFeedbackResponse | null>(null);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  const trimmedLen = message.trim().length;
  const validLength = trimmedLen >= 10 && trimmedLen <= 2000;

  const handleSubmit = async (ev: React.FormEvent) => {
    ev.preventDefault();
    if (trimmedLen < 10) {
      setError(t.minLengthError);
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const response = await submitFeedback({
        category,
        message: message.trim(),
        includeDiagnostics,
      });
      setResult(response);
      if (response.delivered) {
        onNotify(t.deliveredTitle);
        setMessage("");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const resetAndClose = () => {
    setResult(null);
    setError(null);
    onClose();
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="feedback-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 p-4"
    >
      <div className="editorial-card max-h-[90dvh] w-full max-w-xl overflow-y-auto p-5 sm:p-6">
        <div className="mb-4 flex items-start justify-between gap-3 border-b-2 border-[var(--ink)] pb-3">
          <div>
            <p className="eyebrow mb-1">{t.eyebrow}</p>
            <h2 id="feedback-modal-title" className="font-display flex items-center gap-2 text-2xl">
              <MessageSquarePlus className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
              <span>{t.title}</span>
            </h2>
          </div>
          <button
            type="button"
            onClick={resetAndClose}
            aria-label={t.closeBtn}
            className="min-h-[40px] min-w-[40px] cursor-pointer border-2 border-[var(--ink)] bg-[var(--surface)] p-2 text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>

        <p className="mb-4 text-sm text-[var(--muted)]">{t.subtitle}</p>

        {result ? (
          <div className="space-y-4">
            <div className="editorial-callout p-4">
              <div className="mb-1 flex items-center gap-2 text-base font-bold">
                <CheckCircle2 className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
                <span>{result.delivered ? t.deliveredTitle : t.fallbackTitle}</span>
              </div>
              <p className="text-sm text-[var(--muted)]">{result.statusMessage}</p>
            </div>

            <div className="flex flex-wrap gap-3">
              {result.fallbackIssueUrl && (
                <button
                  type="button"
                  onClick={() => void onOpenExternal(result.fallbackIssueUrl!)}
                  className="editorial-btn-primary text-xs"
                >
                  <span>{t.openGithubFallbackBtn}</span>
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                </button>
              )}
              <button type="button" onClick={resetAndClose} className="editorial-btn-secondary text-xs">
                {t.closeBtn}
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={(e) => void handleSubmit(e)} className="space-y-4">
            <div>
              <div className="mb-2 text-xs font-bold uppercase font-mono-editorial text-[var(--muted)]">
                {t.categoryLabel}
              </div>
              <div className="grid gap-2 sm:grid-cols-3">
                {(
                  [
                    { id: "suggestion", label: t.categories.suggestion },
                    { id: "bugReport", label: t.categories.bugReport },
                    { id: "brokenProvider", label: t.categories.brokenProvider },
                  ] as const
                ).map((item) => {
                  const active = category === item.id;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => setCategory(item.id)}
                      className={`min-h-[44px] cursor-pointer border-2 border-[var(--ink)] px-3 py-2 text-xs font-bold ${
                        active
                          ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                          : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                      }`}
                    >
                      {item.label}
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <label
                htmlFor="feedback-message-input"
                className="mb-1.5 block font-mono-editorial text-xs font-bold uppercase text-[var(--muted)]"
              >
                {t.messageLabel}
              </label>
              <textarea
                id="feedback-message-input"
                rows={5}
                maxLength={2000}
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder={t.messagePlaceholder}
                className="w-full border-2 border-[var(--ink)] bg-[var(--paper)] p-3 text-sm text-[var(--ink)] focus:outline-none"
              />
              <div className="mt-1 font-mono-editorial text-[11px] text-[var(--muted)]">
                {t.charsCount(trimmedLen)}
              </div>
            </div>

            <label className="editorial-box-thin flex cursor-pointer items-start gap-3 p-3">
              <input
                type="checkbox"
                checked={includeDiagnostics}
                onChange={(e) => setIncludeDiagnostics(e.target.checked)}
                className="mt-0.5 h-4 w-4 accent-[var(--red)]"
              />
              <span>
                <span className="block text-xs font-bold">{t.diagnosticsLabel}</span>
                <span className="block text-xs text-[var(--muted)]">{t.diagnosticsHint}</span>
              </span>
            </label>

            {error && (
              <div className="border-2 border-[var(--red)] bg-[var(--surface)] p-3 text-xs font-bold text-[var(--red-text)]">
                {error}
              </div>
            )}

            <div className="flex flex-wrap items-center justify-end gap-3 pt-1">
              <button type="button" onClick={resetAndClose} className="editorial-btn-secondary text-xs">
                {t.closeBtn}
              </button>
              <button
                type="submit"
                disabled={submitting || !validLength}
                className="editorial-btn-primary text-xs"
              >
                <Send className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{submitting ? t.submittingBtn : t.submitBtn}</span>
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
