// SPEC: book-reader (READ-12, READ-15, READ-16, READ-17, READ-28)

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronLeft, ChevronRight } from "lucide-react";
import { useReaderStore } from "../../store/readerStore";
import { readerApi } from "../../lib/readerApi";
import type { BookLanguage } from "../../types";

// `null` is "read the original" on the whole boundary, but a <select> value is
// always a string — this is the only place the two representations meet.
const ORIGINAL = "original";

export function ReaderPanel() {
  const { t } = useTranslation();
  const { bookId, page, pageCount, text, pageLanguage, language, isLoading, error, goToPage, setLanguage } =
    useReaderStore();
  const [languages, setLanguages] = useState<BookLanguage[]>([]);

  useEffect(() => {
    if (!bookId) {
      setLanguages([]);
      return;
    }
    // `list_book_languages` counts the files on disk and already includes
    // `original`, so no second call is needed to know what can be read.
    readerApi.listBookLanguages(bookId).then(setLanguages).catch(() => setLanguages([]));
  }, [bookId]);

  useEffect(() => {
    if (!bookId) return;
    function onKeyDown(event: KeyboardEvent) {
      // The language <select> uses the arrows to change option: stealing them
      // there would move the page instead of the choice.
      if (event.target instanceof HTMLSelectElement) return;
      if (event.key === "ArrowLeft") void goToPage(page - 1);
      else if (event.key === "ArrowRight") void goToPage(page + 1);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [bookId, page, goToPage]);

  if (!bookId) {
    return (
      <div className="flex flex-1 items-center justify-center bg-[var(--bg-app)] text-sm text-[var(--text-secondary)]">
        {t("reader.noBookOpen")}
      </div>
    );
  }

  // The index is the same in every language — the pagination comes from
  // `original/` — so switching keeps the current page for free (READ-28).
  const showingOriginalInstead = language !== null && pageLanguage !== language;

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-[var(--bg-app)] text-[var(--text-primary)]">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] px-6 py-4">
        <select
          value={language ?? ORIGINAL}
          onChange={(e) => void setLanguage(e.target.value === ORIGINAL ? null : e.target.value)}
          className="rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-2 py-1 text-sm"
          title={t("reader.language")}
        >
          {languages.map((lang) => (
            <option key={lang.language} value={lang.language}>
              {lang.language} ({lang.pages})
            </option>
          ))}
        </select>

        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={() => void goToPage(page - 1)}
            disabled={page <= 0 || isLoading}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            title={t("reader.previousPage")}
          >
            <ChevronLeft size={18} />
          </button>
          <span className="text-xs text-[var(--text-secondary)]">
            {/* Base 0 on the boundary, base 1 on screen: the shift happens here
                and nowhere else. */}
            {t("reader.pageOf", { page: page + 1, count: pageCount })}
          </span>
          <button
            onClick={() => void goToPage(page + 1)}
            disabled={page >= pageCount - 1 || isLoading}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            title={t("reader.nextPage")}
          >
            <ChevronRight size={18} />
          </button>
        </div>
      </div>

      {/* Silence here would hand the user the original and let them believe it
          is the translation they asked for (READ-12). */}
      {showingOriginalInstead && (
        <p className="flex items-center gap-1.5 border-b border-[var(--border-color)] px-6 py-2 text-xs text-amber-500">
          <AlertTriangle size={14} />
          {t("reader.showingOriginal", { language })}
        </p>
      )}

      {error && <p className="px-6 py-2 text-xs text-red-500">{error}</p>}

      <div className="flex-1 overflow-y-auto px-6 py-8">
        <p className="mx-auto w-full max-w-2xl whitespace-pre-wrap text-[15px] leading-7">{text}</p>
      </div>
    </div>
  );
}
