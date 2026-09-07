// SPEC: book-reader (READ-12, READ-16, READ-17), reading-history (HIST-05, HIST-06, HIST-09),
//       epub-fidelity (FID-02)

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readerApi } from "../lib/readerApi";
import { useUiStore } from "./uiStore";
import type { BookStatusEvent } from "../types";

/** Turning five pages fast must cost one write, not five (READ-17). */
const SAVE_POSITION_DELAY_MS = 800;
let saveTimer: number | undefined;

interface ReaderState {
  bookId: string | null;
  /** Zero-based, the same index the backend stores and clamps. */
  page: number;
  pageCount: number;
  text: string;
  /** The language the text on screen actually came from. Differs from
   *  `language` while that page's translation has not been written yet
   *  (READ-12), and the screen is what says so. */
  pageLanguage: string;
  /** `"html"` = `text` is a whole document for the reader's iframe (FID-02). */
  pageFormat: string;
  /** The language being read; null = `original/`. */
  language: string | null;
  isLoading: boolean;
  error: string | null;

  /** `language` omitted = resolve it from disk; `null` = read the original. */
  openBook: (bookId: string, language?: string | null) => Promise<void>;
  goToPage: (page: number) => Promise<void>;
  setLanguage: (language: string | null) => Promise<void>;
  /** `save: false` drops the position instead of flushing it (HIST-09). */
  closeBook: (save?: boolean) => void;
}

export const useReaderStore = create<ReaderState>((set, get) => ({
  bookId: null,
  page: 0,
  pageCount: 0,
  text: "",
  pageLanguage: "original",
  pageFormat: "txt",
  language: null,
  isLoading: false,
  error: null,

  openBook: async (bookId, language) => {
    // The route switch lives here, not in the callers: opening a book always
    // means going to the reader, and a caller that forgot it shipped a dead
    // button - the Library loaded the book into this store and left the user
    // staring at the Library, because that is the one screen the ReaderPanel is
    // not mounted on.
    //
    // Before the await, on purpose: the click has to paint now. The reader owns
    // the wait from here - it renders its header, disables the arrows while
    // `isLoading`, and shows `error` if the page never arrives.
    useUiStore.getState().setActiveView("reader");
    set({ isLoading: true, error: null, bookId, language: language ?? null, text: "" });
    try {
      // Omitted means the caller does not know the reading language: the
      // sidebar's history row has no such column, and `get_book_page` with
      // `null` reads `original/` - it does not consult the book's
      // `reading_language` (T8 decision 3). Resolving it HERE, after the view
      // switch above, is what keeps the click instant: doing it in the caller
      // put an invoke in front of the navigation.
      const resolved =
        language === undefined
          ? ((await readerApi.listBookLanguages(bookId)).find((l) => l.reading)?.language ?? null)
          : language;
      // The backend owns the clamp and records the opening instant (HIST-04),
      // so the position is asked for, never guessed from the row.
      const page = await readerApi.openBook(bookId);
      const p = await readerApi.getBookPage(bookId, page, resolved);
      set({
        language: resolved,
        page: p.page,
        pageCount: p.page_count,
        text: p.text,
        pageLanguage: p.language,
        pageFormat: p.format,
        isLoading: false,
      });
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  goToPage: async (page) => {
    const { bookId, pageCount, language } = get();
    if (!bookId || page < 0 || page >= pageCount) return;
    set({ isLoading: true, error: null });
    try {
      const p = await readerApi.getBookPage(bookId, page, language);
      set({
        page: p.page,
        pageCount: p.page_count,
        text: p.text,
        pageLanguage: p.language,
        pageFormat: p.format,
        isLoading: false,
      });
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => {
        readerApi.saveReadingPosition(bookId, page).catch(() => {
          // A lost position is cheaper than a banner over the text: the next
          // page turn writes again, and reopening only loses one page.
        });
      }, SAVE_POSITION_DELAY_MS);
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  setLanguage: async (language) => {
    const { bookId, page } = get();
    if (!bookId) return;
    try {
      // Persisted first: the choice must survive closing the book, and the
      // command deletes nothing (READ-27).
      await readerApi.setReadingLanguage(bookId, language);
      set({ language });
      await get().goToPage(page);
    } catch (err) {
      set({ error: String(err) });
    }
  },

  closeBook: (save = true) => {
    const { bookId, page } = get();
    window.clearTimeout(saveTimer);
    // The debounce still pending is exactly the last page turn, the one worth
    // keeping — so closing flushes it instead of cancelling it. The exception
    // is deleting the open book from the history (HIST-09): flushing there
    // would write back the position the deletion just cleared.
    if (save && bookId) void readerApi.saveReadingPosition(bookId, page).catch(() => {});
    set({
      bookId: null,
      page: 0,
      pageCount: 0,
      text: "",
      pageLanguage: "original",
      pageFormat: "txt",
      language: null,
      error: null,
    });
  },
}));

// Same shape as the `document-status` listener in documentsStore: progress
// never comes back through the command's return value. This one serves only the
// open book — the library rows have their own listener in libraryStore.
listen<BookStatusEvent>("book-status", (event) => {
  const { id, status, language, done } = event.payload;
  const state = useReaderStore.getState();
  // The translation of the page on screen just landed: swap the original the
  // reader is showing for the text that was asked for (READ-12). `done` counts
  // pages and `page` is base 0, so `page < done` means this page now exists.
  if (
    id === state.bookId &&
    status === "ready" &&
    language !== null &&
    language === state.language &&
    state.pageLanguage !== language &&
    state.page < done
  ) {
    void state.goToPage(state.page);
  }
});
