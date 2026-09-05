// SPEC: book-library (LIB-03, LIB-09, LIB-10, LIB-11, LIB-12)

import { invoke } from "@tauri-apps/api/core";
import type { BookRecord, ImportBooksResult } from "../types";

export const libraryApi = {
  importBooks: (paths: string[]) => invoke<ImportBooksResult>("import_books", { paths }),
  listBooks: () => invoke<BookRecord[]>("list_books"),
  deleteBook: (id: string) => invoke<void>("delete_book", { id }),
  // Returns the path; opening it is the caller's job via `openPath()` — one
  // command serves both the folder button (LIB-11) and the visible path (LIB-12).
  libraryPath: () => invoke<string>("library_path"),
};
