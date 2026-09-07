// SPEC: book-reader (READ-12, READ-16, READ-17), reading-history (HIST-05, HIST-06)

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { readerApi } from "../lib/readerApi";
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
  /** The language being read; null = `original/`. */
  language: string | null;
  isLoading: boolean;
  error: string | null;

  openBook: (bookId: string, language: string | null) => Promise<void>;
  goToPage: (page: number) => Promise<void>;
  setLanguage: (language: string | null) => Promise<void>;
  closeBook: () => void;
}

export const useReaderStore = create<ReaderState>((set, get) => ({
  bookId: null,
  page: 0,
  pageCount: 0,
  text: "",
  pageLanguage: "original",
  language: null,
  isLoading: false,
  error: null,

  openBook: async (bookId, language) => {
    set({ isLoading: true, error: null, bookId, language, text: "" });
    try {
      // The backend owns the clamp and records the opening instant (HIST-04),
      // so the position is asked for, never guessed from the row.
      const page = await readerApi.openBook(bookId);
      const p = await readerApi.getBookPage(bookId, page, language);
      set({
        page: p.page,
        pageCount: p.page_count,
        text: p.text,
        pageLanguage: p.language,
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

  closeBook: () => {
    const { bookId, page } = get();
    window.clearTimeout(saveTimer);
    // The debounce still pending is exactly the last page turn, the one worth
    // keeping — so closing flushes it instead of cancelling it.
    if (bookId) void readerApi.saveReadingPosition(bookId, page).catch(() => {});
    set({
      bookId: null,
      page: 0,
      pageCount: 0,
      text: "",
      pageLanguage: "original",
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
