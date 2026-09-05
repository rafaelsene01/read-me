// SPEC: book-library (LIB-09, LIB-10)

import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import type { BookRecord } from "../../types";

interface Props {
  book: BookRecord;
  onRemove: () => void;
}

// Same local helper as DocumentRow/ModelsList: this base keeps one copy per
// component instead of a shared util, and T6 is not the task that changes that.
function formatSize(bytes: number) {
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} KB`;
  return `${bytes} B`;
}

export function BookRow({ book, onRemove }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-[var(--border-color)] px-3 py-2">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{book.filename}</p>
        <p className="text-xs text-[var(--text-secondary)]">
          {book.format.toUpperCase()} · {formatSize(book.size_bytes)}
        </p>
      </div>
      <button
        onClick={onRemove}
        className="shrink-0 rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
        title={t("library.remove")}
      >
        <Trash2 size={14} />
      </button>
    </div>
  );
}
