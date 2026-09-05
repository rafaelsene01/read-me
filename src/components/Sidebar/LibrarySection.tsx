// SPEC: book-library (LIB-09)

import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Library } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useLibraryStore } from "../../store/libraryStore";

export function LibrarySection() {
  const { t } = useTranslation();
  const { activeView, setActiveView } = useUiStore();
  const { books, loadBooks } = useLibraryStore();
  const isActive = activeView === "library";

  useEffect(() => {
    loadBooks();
  }, [loadBooks]);

  // Unlike DocumentsSection there is no `ready` filter: a book has no
  // indexing status, so every imported book counts.
  const count = books.length;

  return (
    <div className="border-t border-[var(--border-color)] px-2 py-2">
      <button
        onClick={() => setActiveView("library")}
        className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm ${
          isActive
            ? "bg-[var(--bg-elevated)] text-[var(--text-primary)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]/60 hover:text-[var(--text-primary)]"
        }`}
      >
        <Library size={14} />
        <span className="flex-1 text-left">{t("sidebar.library")}</span>
        {count > 0 && (
          <span className="rounded-full bg-[var(--bg-elevated)] px-1.5 text-xs text-[var(--text-secondary)]">
            {count}
          </span>
        )}
      </button>
    </div>
  );
}
