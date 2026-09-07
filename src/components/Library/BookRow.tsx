// SPEC: book-library (LIB-09, LIB-10), book-reader (READ-01, READ-02, READ-03, READ-04, READ-05)

import { useTranslation } from "react-i18next";
import { BookOpen, Pencil, Play, Trash2, X } from "lucide-react";
import type { BookRecord, BookStatus, BookStatusEvent } from "../../types";

// What the reader can extract text from: pdfium for PDF, the zip/spine path for
// EPUB. The three PalmDB formats stay importable (LIB-01) and say so in the row
// instead of offering a button that would fail on click (READ-03).
const READABLE_FORMATS = ["pdf", "epub"];

const STATUS_LABEL_KEY: Record<BookStatus, string> = {
  imported: "library.statusImported",
  extracting: "library.statusExtracting",
  paginating: "library.statusPaginating",
  ready: "library.statusReady",
  error: "library.statusError",
};

interface Props {
  book: BookRecord;
  /** Last `book-status` event for this book; the only sign of translation. */
  progress?: BookStatusEvent;
  isBusy: boolean;
  isEditing: boolean;
  onRemove: () => void;
  onProcess: () => void;
  onCancel: () => void;
  onRead: () => void;
  onEdit: () => void;
}

// Same local helper as DocumentRow/ModelsList: this base keeps one copy per
// component instead of a shared util, and T6 is not the task that changes that.
function formatSize(bytes: number) {
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} KB`;
  return `${bytes} B`;
}

export function BookRow({
  book,
  progress,
  isBusy,
  isEditing,
  onRemove,
  onProcess,
  onCancel,
  onRead,
  onEdit,
}: Props) {
  const { t } = useTranslation();
  const readable = READABLE_FORMATS.includes(book.format);
  const isReady = book.status === "ready" && book.page_count > 0;
  // `translating` is deliberately not a book status — translating is work per
  // language — so the bar comes from the event: `language` set, `done` absolute
  // against `total` so a resumed run does not restart at zero (READ-30).
  const translation =
    isBusy && progress && progress.language !== null && progress.total > 0 ? progress : null;

  return (
    <div className="rounded-md border border-[var(--border-color)] px-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{book.filename}</p>
          <p className="text-xs text-[var(--text-secondary)]">
            {book.format.toUpperCase()} · {formatSize(book.size_bytes)} ·{" "}
            {!readable
              ? t("library.statusUnsupported")
              : isReady
                ? t("library.pages", { pages: book.page_count })
                : t(STATUS_LABEL_KEY[book.status])}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {isBusy ? (
            <button
              onClick={onCancel}
              className="flex items-center gap-1.5 rounded-md border border-[var(--border-color)] px-2 py-1 text-xs hover:bg-[var(--bg-elevated)]"
            >
              <X size={14} />
              {t("library.cancel")}
            </button>
          ) : isReady ? (
            <>
              <button
                onClick={onRead}
                className="flex items-center gap-1.5 rounded-md bg-[var(--accent)] px-2 py-1 text-xs font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)]"
              >
                <BookOpen size={14} />
                {t("library.read")}
              </button>
              <button
                onClick={onEdit}
                aria-expanded={isEditing}
                className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
                title={t("library.edit")}
              >
                <Pencil size={14} />
              </button>
            </>
          ) : (
            // READ-03: no button at all for MOBI/AZW/AZW3 — the label above is
            // what the row says instead, and it is readable without clicking.
            readable && (
              <button
                onClick={onProcess}
                className="flex items-center gap-1.5 rounded-md bg-[var(--accent)] px-2 py-1 text-xs font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)]"
              >
                <Play size={14} />
                {t("library.process")}
              </button>
            )
          )}
          <button
            onClick={onRemove}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
            title={t("library.remove")}
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {isBusy && (
        <div className="mt-2">
          {translation && (
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-[var(--bg-elevated)]">
              <div
                className="h-full bg-[var(--accent)] transition-all"
                style={{ width: `${Math.min(100, Math.round((translation.done / translation.total) * 100))}%` }}
              />
            </div>
          )}
          <p className="mt-1 text-xs text-[var(--text-secondary)]">
            {translation
              ? t("library.translating", {
                  language: translation.language,
                  done: translation.done,
                  total: translation.total,
                })
              : t("library.processing")}
          </p>
        </div>
      )}

      {/* A failed translation leaves the book `ready` on purpose (T6), so this
          line only ever describes extraction or pagination. */}
      {book.status === "error" && book.error_message && (
        <p className="mt-1 text-xs text-red-500">{book.error_message}</p>
      )}
    </div>
  );
}
