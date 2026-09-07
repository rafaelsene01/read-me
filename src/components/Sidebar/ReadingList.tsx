// SPEC: reading-history (HIST-01, HIST-02, HIST-03, HIST-06), book-reader (READ-16)

import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookOpen } from "lucide-react";
import { readerApi } from "../../lib/readerApi";
import { useReaderStore } from "../../store/readerStore";
import { useUiStore } from "../../store/uiStore";
import type { ReadingEntry } from "../../types";

export function ReadingList() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<ReadingEntry[]>([]);
  const openBookId = useReaderStore((s) => s.bookId);
  const openBook = useReaderStore((s) => s.openBook);
  const setActiveView = useUiStore((s) => s.setActiveView);

  // The backend already orders by `last_opened_at DESC` (HIST-02); reloading
  // after an open is what keeps the list in that order on screen.
  const load = useCallback(() => {
    readerApi.listReadingHistory().then(setEntries).catch(() => {
      // An unreadable history is an empty sidebar, not a banner over the app:
      // the Library is still one click below and imports still work.
    });
  }, []);

  useEffect(load, [load]);

  async function handleOpen(entry: ReadingEntry) {
    // `get_book_page` with `null` reads `original/` — it does not consult the
    // book's `reading_language` (T8 decision 3), and `ReadingEntry` does not
    // carry it. So the language comes from the one command that already knows
    // which folder is being read, instead of a new column on the history row.
    const languages = await readerApi.listBookLanguages(entry.id);
    await openBook(entry.id, languages.find((l) => l.reading)?.language ?? null);
    setActiveView("reader");
    load();
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-2 py-1">
      <p className="px-2 py-1 text-xs font-medium text-[var(--text-secondary)]">
        {t("reader.history")}
      </p>
      {entries.length === 0 ? (
        // HIST-03: an empty history says where books come from.
        <p className="px-2 py-1 text-xs text-[var(--text-secondary)]">{t("reader.historyEmpty")}</p>
      ) : (
        <ul className="space-y-0.5">
          {entries.map((entry) => (
            <li key={entry.id}>
              <button
                onClick={() => void handleOpen(entry)}
                className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-[var(--bg-elevated)] ${
                  entry.id === openBookId ? "bg-[var(--bg-elevated)]" : ""
                }`}
              >
                <BookOpen size={14} className="shrink-0 text-[var(--text-secondary)]" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{entry.filename}</span>
                  <span className="block text-xs text-[var(--text-secondary)]">
                    {t("reader.pageOf", { page: entry.last_page + 1, count: entry.page_count })}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
