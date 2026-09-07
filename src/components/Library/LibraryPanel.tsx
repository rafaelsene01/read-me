// SPEC: book-library (LIB-01, LIB-03, LIB-04, LIB-09, LIB-10, LIB-11, LIB-12),
//       book-reader (READ-02, READ-04, READ-05, READ-06)

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { ArrowLeft, FolderOpen, Upload } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useLibraryStore } from "../../store/libraryStore";
import { useReaderStore } from "../../store/readerStore";
import { BookRow } from "./BookRow";
import { BookEditPanel } from "./BookEditPanel";
import { ProcessDialog } from "./ProcessDialog";
import type { BookRecord } from "../../types";

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
    progress,
    loadBooks,
    loadLibraryPath,
    importBooks,
    deleteBook,
    processBook,
    cancelProcessing,
  } = useLibraryStore();
  const openBook = useReaderStore((s) => s.openBook);
  const [dialogBook, setDialogBook] = useState<BookRecord | null>(null);
  const [editingBookId, setEditingBookId] = useState<string | null>(null);
  /** Ids of the books with a run in flight. There is no `translating` status to
   *  read this from: the run's end is the `processBook` promise resolving. */
  const [busyIds, setBusyIds] = useState<string[]>([]);

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

  async function handleProcess(book: BookRecord, language: string | null) {
    setDialogBook(null);
    setBusyIds((ids) => [...ids, book.id]);
    // A translation that fails leaves the book `ready` and its pages on disk
    // (T6): the error lands in the store's banner and the row stays in the list.
    await processBook(book.id, language);
    setBusyIds((ids) => ids.filter((id) => id !== book.id));
  }

  return (
    <div className="flex flex-1 flex-col overflow-y-auto bg-[var(--bg-app)] text-[var(--text-primary)]">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] px-6 py-4">
        <button
          onClick={() => setActiveView("reader")}
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
              <div key={book.id}>
                <BookRow
                  book={book}
                  progress={progress[book.id]}
                  isBusy={busyIds.includes(book.id)}
                  isEditing={editingBookId === book.id}
                  onRemove={() => deleteBook(book.id)}
                  onProcess={() => setDialogBook(book)}
                  onCancel={() => void cancelProcessing(book.id)}
                  // Loads the book into the reader store; the `reader` view that
                  // renders it is routed by T11, which owns uiStore and App.tsx.
                  onRead={() => void openBook(book.id, book.reading_language)}
                  onEdit={() => setEditingBookId(editingBookId === book.id ? null : book.id)}
                />
                {/* READ-24: the panel opens under its own row, so the book it
                    edits is never in doubt. */}
                {editingBookId === book.id && (
                  <BookEditPanel book={book} onClose={() => setEditingBookId(null)} />
                )}
              </div>
            ))
          )}
        </div>
      </div>

      {dialogBook && (
        <ProcessDialog
          book={dialogBook}
          onClose={() => setDialogBook(null)}
          onConfirm={(language) => void handleProcess(dialogBook, language)}
        />
      )}
    </div>
  );
}
