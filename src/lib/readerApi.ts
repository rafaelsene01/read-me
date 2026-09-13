// SPEC: book-reader (READ-02, READ-05, READ-16, READ-17, READ-26, READ-27,
//       READ-29, READ-30), reading-history (HIST-02, HIST-09, HIST-10, HIST-11),
//       book-illustrations (ILLUS-07)

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
  /** A new reading at the first page; returns its id (HIST-10). */
  startReading: (bookId: string) => invoke<string>("start_reading", { bookId }),
  /** Returns the position to resume at, already clamped (READ-16). */
  openReading: (readingId: string) => invoke<number>("open_reading", { readingId }),
  saveReadingPosition: (readingId: string, page: number) =>
    invoke<void>("save_reading_position", { readingId, page }),
  getBookPage: (bookId: string, page: number, language: string | null) =>
    invoke<BookPage>("get_book_page", { bookId, page, language }),
  listReadingHistory: () => invoke<ReadingEntry[]>("list_reading_history"),
  /** Raw bytes, not a path: the asset protocol is disabled (ILLUS-07). */
  getBookImage: (bookId: string, name: string) =>
    invoke<ArrayBuffer>("get_book_image", { bookId, name }),
  /** Deletes one reading. Deletes nothing on disk (HIST-09). */
  forgetReadingEntry: (readingId: string) =>
    invoke<void>("forget_reading_entry", { readingId }),
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
