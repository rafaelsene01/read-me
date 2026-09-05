// SPEC: book-library (LIB-01, LIB-03, LIB-04, LIB-09, LIB-10, LIB-11, LIB-12)

import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { ArrowLeft, FolderOpen, Upload } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useLibraryStore } from "../../store/libraryStore";
import { BookRow } from "./BookRow";

const BOOK_EXTENSIONS = ["pdf", "epub", "mobi", "azw", "azw3"];

export function LibraryPanel() {
  const { t } = useTranslation();
  const setActiveView = useUiStore((s) => s.setActiveView);
  const {
    books,
    rejected,
    libraryPath,
    isImporting,
    error,
    loadBooks,
    loadLibraryPath,
    importBooks,
    deleteBook,
  } = useLibraryStore();

  useEffect(() => {
    loadBooks();
    loadLibraryPath();
  }, [loadBooks, loadLibraryPath]);

  async function handleImport() {
    const selected = await open({
      multiple: true,
      title: t("library.fileDialogTitle"),
      filters: [{ name: t("library.supportedFormats"), extensions: BOOK_EXTENSIONS }],
    });
    if (!selected) return;
    await importBooks(Array.isArray(selected) ? selected : [selected]);
  }

  return (
    <div className="flex flex-1 flex-col overflow-y-auto bg-[var(--bg-app)] text-[var(--text-primary)]">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] px-6 py-4">
        <button
          onClick={() => setActiveView("chat")}
          className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
          title={t("settings.back")}
        >
          <ArrowLeft size={18} />
        </button>
        <h1 className="text-base font-semibold">{t("library.title")}</h1>
      </div>

      <div className="mx-auto w-full max-w-2xl px-6 py-6">
        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={handleImport}
            disabled={isImporting}
            className="flex items-center gap-1.5 rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)] disabled:opacity-50"
          >
            <Upload size={14} />
            {isImporting ? t("library.importing") : t("library.import")}
          </button>
          <button
            onClick={() => libraryPath && openPath(libraryPath)}
            disabled={!libraryPath}
            className="flex items-center gap-1.5 rounded-md border border-[var(--border-color)] px-3 py-1.5 text-sm hover:bg-[var(--bg-elevated)] disabled:opacity-50"
          >
            <FolderOpen size={14} />
            {t("library.openFolder")}
          </button>
          {/* The absolute path sits next to the button, not behind a click (LIB-12). */}
          {libraryPath && (
            <span className="min-w-0 truncate text-xs text-[var(--text-secondary)]" title={libraryPath}>
              {libraryPath}
            </span>
          )}
        </div>

        <p className="mt-1 text-xs text-[var(--text-secondary)]">{t("library.supportedFormats")}</p>

        {/* A whole-command failure (unconfigured storage, LIB-04) must be read, not swallowed. */}
        {error && <p className="mt-3 text-xs text-red-500">{error}</p>}

        {/* Shown alongside the imported books, never instead of them (LIB-03). */}
        {rejected.map((item) => (
          <p key={item.path} className="mt-2 text-xs text-amber-500">
            {t("library.rejected", {
              name: item.path.split(/[\\/]/).pop() ?? item.path,
              reason: item.reason,
            })}
          </p>
        ))}

        <div className="mt-6 space-y-2">
          {books.length === 0 ? (
            <p className="text-sm text-[var(--text-secondary)]">{t("library.empty")}</p>
          ) : (
            books.map((book) => (
              <BookRow key={book.id} book={book} onRemove={() => deleteBook(book.id)} />
            ))
          )}
        </div>
      </div>
    </div>
  );
}
