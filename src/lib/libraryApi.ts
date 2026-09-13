// SPEC: book-library (LIB-03, LIB-09, LIB-10, LIB-11, LIB-13)

import { invoke } from "@tauri-apps/api/core";
import type { BookRecord, ImportBooksResult } from "../types";

export const libraryApi = {
  importBooks: (paths: string[]) => invoke<ImportBooksResult>("import_books", { paths }),
  listBooks: () => invoke<BookRecord[]>("list_books"),
  deleteBook: (id: string) => invoke<void>("delete_book", { id }),
  // The path only enables the button; opening goes through Rust, because
  // `opener:default` does not grant `openPath()` (LIB-11, AD-069).
  libraryPath: () => invoke<string>("library_path"),
  openLibraryFolder: () => invoke<void>("open_library_folder"),
  /** Raw cover bytes, like `get_book_image`. Empty when the book declares none. */
  getBookCover: (bookId: string) => invoke<ArrayBuffer>("get_book_cover", { bookId }),
};
