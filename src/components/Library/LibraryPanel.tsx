// SPEC: book-library (LIB-01, LIB-03, LIB-04, LIB-09, LIB-10, LIB-11, LIB-14, LIB-15),
//       book-reader (READ-02, READ-04, READ-05, READ-06)
// LIB-12 (show absolute path in UI) revoked by AD-066 — path text removed from the header.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { libraryApi } from "../../lib/libraryApi";
import { ArrowLeft, FolderOpen, LayoutGrid, List, Upload } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { useLibraryStore } from "../../store/libraryStore";
import { useReaderStore } from "../../store/readerStore";
import { BookRow } from "./BookRow";
import { BookEditPanel } from "./BookEditPanel";
import { ProcessDialog } from "./ProcessDialog";
import type { BookRecord } from "../../types";

// The reader renders the book's own HTML, CSS and fonts (`epub-fidelity`), and
// nothing else reaches it that way. The dialog filter is only a hint — the gate
// that decides is `is_supported_book` in Rust.
const BOOK_EXTENSIONS = ["epub"];

// LIB-14: list or cards, and the card width in px. A view preference of this
// screen, not app configuration - so it lives in this window's storage, and a
// storage that throws (or is empty) just means the defaults.
const VIEW_KEY = "readme-library-view";
const CARD_SIZE_KEY = "readme-library-card-size";
const CARD_MIN = 100;
const CARD_MAX = 360;
const CARD_STEP = 20;
const CARD_DEFAULT = 180;

function readPref(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writePref(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembering a view choice is not worth a banner.
  }
}

function validCardSize(value: number): boolean {
  return Number.isFinite(value) && value >= CARD_MIN && value <= CARD_MAX;
}

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
  const [view, setView] = useState<"list" | "cards">(() =>
    readPref(VIEW_KEY) === "cards" ? "cards" : "list",
  );
  const [cardSize, setCardSize] = useState(() => {
    const stored = Number(readPref(CARD_SIZE_KEY));
    return validCardSize(stored) ? stored : CARD_DEFAULT;
  });

  useEffect(() => writePref(VIEW_KEY, view), [view]);
  useEffect(() => writePref(CARD_SIZE_KEY, String(cardSize)), [cardSize]);
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

  const editingBook = books.find((book) => book.id === editingBookId) ?? null;

  function bookRow(book: BookRecord, variant: "row" | "card") {
    return (
      <BookRow
        book={book}
        variant={variant}
        progress={progress[book.id]}
        isBusy={busyIds.includes(book.id)}
        isEditing={editingBookId === book.id}
        onRemove={() => deleteBook(book.id)}
        onProcess={() => setDialogBook(book)}
        onCancel={() => void cancelProcessing(book.id)}
        // `openBook` switches to the reader view itself - see the comment on
        // it. This row only says which book.
        onRead={() => void openBook(book.id, book.reading_language)}
        onEdit={() => setEditingBookId(editingBookId === book.id ? null : book.id)}
      />
    );
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
        <div className="ml-auto flex items-center gap-2">
          <div className="flex overflow-hidden rounded-md border border-[var(--border-color)]">
            {(
              [
                ["list", List, "library.viewList"],
                ["cards", LayoutGrid, "library.viewCards"],
              ] as const
            ).map(([option, Icon, labelKey]) => (
              <button
                key={option}
                onClick={() => setView(option)}
                aria-pressed={view === option}
                title={t(labelKey)}
                aria-label={t(labelKey)}
                className={`p-1.5 ${
                  view === option
                    ? "bg-[var(--bg-elevated)] text-[var(--text-primary)]"
                    : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                }`}
              >
                <Icon size={16} />
              </button>
            ))}
          </div>
          {view === "cards" && (
            <label className="flex items-center gap-1.5 text-xs text-[var(--text-secondary)]">
              {t("library.cardSize")}
              {/* `defaultValue`, not `value`: typing 240 passes through 2 and
                  24, and a controlled field would snap those back. Only a value
                  inside the range reaches the grid. */}
              <input
                type="number"
                min={CARD_MIN}
                max={CARD_MAX}
                step={CARD_STEP}
                defaultValue={cardSize}
                onChange={(e) => {
                  const next = Number(e.target.value);
                  if (validCardSize(next)) setCardSize(next);
                }}
                className="w-16 rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-1.5 py-1 text-sm text-[var(--text-primary)]"
              />
              px
            </label>
          )}
          <button
            onClick={handleImport}
            disabled={isImporting}
            className="flex items-center gap-1.5 rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)] disabled:opacity-50"
          >
            <Upload size={14} />
            {isImporting ? t("library.importing") : t("library.import")}
          </button>
          <button
            // A refusal lands in the banner: the old `openPath` call had no
            // catch, which is why the broken button said nothing (AD-069).
            onClick={() =>
              libraryApi
                .openLibraryFolder()
                .catch((e) => useLibraryStore.setState({ error: String(e) }))
            }
            disabled={!libraryPath}
            className="flex items-center gap-1.5 rounded-md border border-[var(--border-color)] px-3 py-1.5 text-sm hover:bg-[var(--bg-elevated)] disabled:opacity-50"
          >
            <FolderOpen size={14} />
            {t("library.openFolder")}
          </button>
        </div>
      </div>

      {/* The list keeps its reading width; cards use the whole panel. */}
      <div className={`mx-auto w-full px-6 py-6 ${view === "list" ? "max-w-2xl" : ""}`}>
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

        {books.length === 0 ? (
          <p className="mt-6 text-sm text-[var(--text-secondary)]">{t("library.empty")}</p>
        ) : view === "list" ? (
          <div className="mt-6 space-y-2">
            {books.map((book) => (
              <div key={book.id}>
                {bookRow(book, "row")}
                {/* READ-24: the panel opens under its own row, so the book it
                    edits is never in doubt. */}
                {editingBookId === book.id && (
                  <BookEditPanel book={book} onClose={() => setEditingBookId(null)} />
                )}
              </div>
            ))}
          </div>
        ) : (
          <>
            <div
              className="mt-6 grid gap-4"
              style={{ gridTemplateColumns: `repeat(auto-fill, ${cardSize}px)` }}
            >
              {books.map((book) => (
                <div key={book.id}>{bookRow(book, "card")}</div>
              ))}
            </div>
            {/* In cards the editor spans the width under the grid: inside a cell
                it would be squeezed to one card. Its card stays turned while it
                is open, which is what keeps READ-24's "which book" answered. */}
            {editingBook && (
              <div className="mt-4">
                <BookEditPanel book={editingBook} onClose={() => setEditingBookId(null)} />
              </div>
            )}
          </>
        )}
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
