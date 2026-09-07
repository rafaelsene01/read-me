// SPEC: book-reader (READ-02, READ-05, READ-16, READ-17, READ-26, READ-27,
//       READ-29, READ-30), reading-history (HIST-02)

import { invoke } from "@tauri-apps/api/core";
import type { BookLanguage, BookPage, ReadingEntry } from "../types";

// Page indexes are base 0 on this whole boundary; the `{:04}.txt` files are
// base 1 and Rust converts once, on its side — nothing here shifts an index.
// `invoke` parameters go camelCase and arrive snake_case: Tauri converts them,
// unlike the struct fields, which serde does not rename.
export const readerApi = {
  /** Returns the page count. `language: null` = extract only, no translation. */
  processBook: (bookId: string, language: string | null) =>
    invoke<number>("process_book", { bookId, language }),
  /** Also cancels a retranslation: same key, there is no second command. */
  cancelProcessing: (bookId: string) => invoke<void>("cancel_processing", { bookId }),
  /** Returns the position to resume at, already clamped (READ-16). */
  openBook: (bookId: string) => invoke<number>("open_book", { bookId }),
  saveReadingPosition: (bookId: string, page: number) =>
    invoke<void>("save_reading_position", { bookId, page }),
  getBookPage: (bookId: string, page: number, language: string | null) =>
    invoke<BookPage>("get_book_page", { bookId, page, language }),
  listReadingHistory: () => invoke<ReadingEntry[]>("list_reading_history"),
  /** `pages: null` redoes the whole language; `[]` only fills what is missing. */
  retranslatePages: (bookId: string, language: string, pages: number[] | null) =>
    invoke<void>("retranslate_pages", { bookId, language, pages }),
  addLanguage: (bookId: string, language: string) =>
    invoke<void>("add_language", { bookId, language }),
  /** Returns how many translated pages were deleted (READ-29). */
  removeLanguage: (bookId: string, language: string) =>
    invoke<number>("remove_language", { bookId, language }),
  /** `null` (or "original") reads the extracted text. Deletes nothing. */
  setReadingLanguage: (bookId: string, language: string | null) =>
    invoke<void>("set_reading_language", { bookId, language }),
  listBookLanguages: (bookId: string) =>
    invoke<BookLanguage[]>("list_book_languages", { bookId }),
};
