// SPEC: book-library (LIB-03, LIB-09, LIB-10, LIB-11, LIB-12)

import { create } from "zustand";
import { libraryApi } from "../lib/libraryApi";
import type { BookRecord, RejectedImport } from "../types";

interface LibraryState {
  books: BookRecord[];
  /** Files refused by the last import, kept next to the ones that went in. */
  rejected: RejectedImport[];
  /** Absolute path of the library folder, shown in the UI (LIB-12). */
  libraryPath: string | null;
  isLoading: boolean;
  isImporting: boolean;
  error: string | null;

  loadBooks: () => Promise<void>;
  loadLibraryPath: () => Promise<void>;
  importBooks: (paths: string[]) => Promise<void>;
  deleteBook: (id: string) => Promise<void>;
}

// Unlike the documents store there is no `document-status` listener: importing
// a book ends when the copy ends, so there is no progress to follow.
export const useLibraryStore = create<LibraryState>((set, get) => ({
  books: [],
  rejected: [],
  libraryPath: null,
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
}));
