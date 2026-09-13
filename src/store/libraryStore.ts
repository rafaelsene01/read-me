// SPEC: book-library (LIB-03, LIB-09, LIB-10, LIB-11),
//       book-reader (READ-02, READ-05)

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { libraryApi } from "../lib/libraryApi";
import { readerApi } from "../lib/readerApi";
import type { BookRecord, BookStatusEvent, RejectedImport } from "../types";

interface LibraryState {
  books: BookRecord[];
  /** Files refused by the last import, kept next to the ones that went in. */
  rejected: RejectedImport[];
  /** Absolute path of the library folder, what "Open folder" opens (LIB-11).
   *  No longer shown on screen: LIB-12 was revoked by AD-066. */
  libraryPath: string | null;
  /** Last `book-status` event per book, for the row's progress (READ-05).
   *  Keyed by book id because several books can be processing at once. */
  progress: Record<string, BookStatusEvent>;
  isLoading: boolean;
  isImporting: boolean;
  error: string | null;

  loadBooks: () => Promise<void>;
  loadLibraryPath: () => Promise<void>;
  importBooks: (paths: string[]) => Promise<void>;
  deleteBook: (id: string) => Promise<void>;
  processBook: (id: string, language: string | null) => Promise<void>;
  cancelProcessing: (id: string) => Promise<void>;
}

// Importing a book still ends when the copy ends; what has progress is
// processing it, and that arrives on `book-status` (see the listener below).
export const useLibraryStore = create<LibraryState>((set, get) => ({
  books: [],
  rejected: [],
  libraryPath: null,
  progress: {},
  isLoading: false,
  isImporting: false,
  error: null,

  loadBooks: async () => {
    set({ isLoading: true, error: null });
    try {
      // Already ordered newest-first by the SQL (LIB-09) — not re-sorted here.
      const books = await libraryApi.listBooks();
      set({ books, isLoading: false });
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  loadLibraryPath: async () => {
    try {
      set({ libraryPath: await libraryApi.libraryPath() });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  importBooks: async (paths) => {
    set({ isImporting: true, error: null, rejected: [] });
    try {
      // A partly valid selection is normal: the accepted files are in, the
      // refused ones come back named and are kept for the UI (LIB-03).
      const { rejected } = await libraryApi.importBooks(paths);
      set({ rejected });
      await get().loadBooks();
    } catch (err) {
      set({ error: String(err) });
    } finally {
      set({ isImporting: false });
    }
  },

  deleteBook: async (id) => {
    try {
      await libraryApi.deleteBook(id);
      set({ books: get().books.filter((b) => b.id !== id) });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  // Resolves only when the whole run ends — extraction, pagination and, if a
  // language was asked for, the translation of every page. The steps in between
  // are the `book-status` events; this promise is just the end of the road.
  processBook: async (id, language) => {
    set({ error: null });
    try {
      await readerApi.processBook(id, language);
    } catch (err) {
      set({ error: String(err) });
    }
    // The listener patches `status`, but `page_count` and the position columns
    // only land in the database, so the list is reread when the run ends.
    await get().loadBooks();
  },

  cancelProcessing: async (id) => {
    try {
      await readerApi.cancelProcessing(id);
    } catch (err) {
      set({ error: String(err) });
    }
  },
}));

// `book-status` is to books what `document-status` is to documents: the
// progress of a run never comes back through the command's return value.
listen<BookStatusEvent>("book-status", (event) => {
  const { id, status, error_message } = event.payload;
  useLibraryStore.setState((state) => ({
    books: state.books.map((b) => (b.id === id ? { ...b, status, error_message } : b)),
    progress: { ...state.progress, [id]: event.payload },
  }));
});
