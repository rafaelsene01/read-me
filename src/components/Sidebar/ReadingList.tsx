// SPEC: reading-history (HIST-01, HIST-02, HIST-03, HIST-06, HIST-09, HIST-10, HIST-11),
//       book-reader (READ-16)

import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BookOpen, Trash2 } from "lucide-react";
import { readerApi } from "../../lib/readerApi";
import { useReaderStore } from "../../store/readerStore";
import type { ReadingEntry } from "../../types";

export function ReadingList() {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<ReadingEntry[]>([]);
  const openReadingId = useReaderStore((s) => s.readingId);
  const openBook = useReaderStore((s) => s.openBook);
  const closeBook = useReaderStore((s) => s.closeBook);

  // The backend already orders by `last_opened_at DESC` (HIST-02); reloading
  // after an open is what keeps the list in that order on screen.
  const load = useCallback(() => {
    readerApi.listReadingHistory().then(setEntries).catch(() => {
      // An unreadable history is an empty sidebar, not a banner over the app:
      // the Library is still one click below and imports still work.
    });
  }, []);

  // Reloaded whenever the open reading changes: "Ler" in the Library creates
  // an entry this list never clicked, and it has to show up (HIST-10).
  useEffect(load, [load, openReadingId]);

  async function handleRemove(entry: ReadingEntry) {
    // HIST-09: the position is lost, so the question says it instead of the row
    // vanishing silently. Same `window.confirm` the Library uses to drop a
    // language - this base has no modal system and does not need one here.
    if (!window.confirm(t("reader.historyRemoveConfirm", { name: entry.filename }))) return;
    // Closing without the flush first: the pending debounce of the book being
    // read would write the position straight back after it was cleared.
    if (entry.id === openReadingId) closeBook(false);
    try {
      await readerApi.forgetReadingEntry(entry.id);
    } catch {
      // Nothing was deleted, so nothing to undo: the row stays and the reload
      // below shows it is still there.
    }
    load();
  }

  async function handleOpen(entry: ReadingEntry) {
    // No language argument: a history row does not carry the reading language,
    // and the store resolves it from disk after it has already switched to the
    // reader - so the click paints immediately.
    await openBook(entry.book_id, undefined, entry.id);
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
              {/* The delete button is a sibling of the open button, not inside
                  it: a button nested in a button is invalid HTML. Same layout
                  the Library rows use. */}
              <div
                className={`flex items-center rounded-md pr-1 hover:bg-[var(--bg-elevated)] ${
                  entry.id === openReadingId ? "bg-[var(--bg-elevated)]" : ""
                }`}
              >
                <button
                  onClick={() => void handleOpen(entry)}
                  className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm"
                >
                  <BookOpen size={14} className="shrink-0 text-[var(--text-secondary)]" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{entry.filename}</span>
                    <span className="block text-xs text-[var(--text-secondary)]">
                      {t("reader.pageOf", { page: entry.last_page + 1, count: entry.page_count })}
                    </span>
                  </span>
                </button>
                <button
                  onClick={() => void handleRemove(entry)}
                  title={t("reader.historyRemove")}
                  aria-label={t("reader.historyRemove")}
                  className="shrink-0 rounded-md p-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
