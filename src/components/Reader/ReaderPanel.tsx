// SPEC: book-reader (READ-12, READ-15, READ-16, READ-17, READ-28),
//       book-illustrations (ILLUS-06), epub-fidelity (FID-02, FID-04),
//       read-aloud (TTS-03, TTS-05, TTS-12, TTS-17, TTS-33, TTS-34)

import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronLeft, ChevronRight, Pause, Play, Square } from "lucide-react";
import { useReaderStore } from "../../store/readerStore";
import { readerApi } from "../../lib/readerApi";
import { READER_SCRIPT, READER_SCRIPT_CSS } from "../../lib/readerScript";
import { useReadAloudStore } from "../../store/readAloudStore";
import type { BookLanguage } from "../../types";

// `null` is "read the original" on the whole boundary, but a <select> value is
// always a string — this is the only place the two representations meet.
const ORIGINAL = "original";

// The marker the extractors leave in the text, as a whole paragraph
// (ILLUS-04). The shape is duplicated from Rust on purpose: it crosses the
// boundary as plain text, so there is nothing generated to share, and the
// definition lives in `reader/illustrations.rs`.
const IMAGE_MARKER = /^\[\[image: (\d{4}\.[a-z0-9]{1,5})\]\]$/;

/** A page split into what to render: prose, or the name of an illustration. */
function blocksOf(text: string): { text: string; image?: string }[] {
  return text
    .split(/\n{2,}/)
    .filter((block) => block.trim() !== "")
    .map((block) => {
      const match = IMAGE_MARKER.exec(block.trim());
      return match ? { text: block, image: match[1] } : { text: block };
    });
}

/** One illustration, fetched as bytes because the asset protocol is off. */
function Illustration({ bookId, name }: { bookId: string; name: string }) {
  const { t } = useTranslation();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;
    setFailed(false);
    readerApi
      .getBookImage(bookId, name)
      .then((bytes) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(new Blob([bytes]));
        setUrl(objectUrl);
      })
      .catch(() => !cancelled && setFailed(true));
    // Revoking is the whole reason this is a component: turning pages without
    // it leaks one `blob:` per illustration for the life of the window.
    return () => {
      cancelled = true;
      setUrl(null);
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [bookId, name]);

  if (failed) {
    return (
      <p className="text-xs text-[var(--text-secondary)]">{t("reader.imageMissing")}</p>
    );
  }
  // The empty box before the bytes arrive keeps the text from jumping when
  // they do.
  return (
    <div className="flex min-h-24 justify-center">
      {url && (
        <img
          src={url}
          alt={t("reader.illustration")}
          className="max-h-[70vh] max-w-full rounded-sm object-contain"
        />
      )}
    </div>
  );
}

export function ReaderPanel() {
  const { t } = useTranslation();
  const {
    bookId,
    page,
    pageCount,
    text,
    pageFormat,
    pageLanguage,
    language,
    isLoading,
    error,
    goToPage,
    setLanguage,
  } = useReaderStore();
  const [languages, setLanguages] = useState<BookLanguage[]>([]);
  const blocks = useMemo(() => blocksOf(text), [text]);
  const aloud = useReadAloudStore();

  // The app's own script goes in just before `</body>`, so it lives in TS
  // where it is readable, and the document the backend assembled stays the
  // book's (FID-02). The page is already sanitized on disk, which is what
  // makes `allow-scripts` safe to hand it.
  const srcDoc = useMemo(() => {
    if (pageFormat !== "html") return "";
    const injected = `<style>${READER_SCRIPT_CSS}</style><script>${READER_SCRIPT}</script>`;
    return text.includes("</body>")
      ? text.replace("</body>", `${injected}</body>`)
      : text + injected;
  }, [text, pageFormat]);

  // The page talks back through `postMessage` and nothing else: the frame has
  // an opaque origin, so this is the only channel it has (TTS-12).
  useEffect(() => {
    function onMessage(event: MessageEvent) {
      const data = event.data as { readaloud?: string; block?: number; offset?: number };
      if (data?.readaloud !== "click") return;
      void useReadAloudStore.getState().startAt(data.block ?? 0, data.offset ?? 0);
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);

  useEffect(() => {
    if (!bookId) {
      setLanguages([]);
      return;
    }
    // `list_book_languages` counts the files on disk and already includes
    // `original`, so no second call is needed to know what can be read.
    readerApi.listBookLanguages(bookId).then(setLanguages).catch(() => setLanguages([]));
  }, [bookId]);

  useEffect(() => {
    if (!bookId) return;
    function onKeyDown(event: KeyboardEvent) {
      // The language <select> uses the arrows to change option: stealing them
      // there would move the page instead of the choice.
      if (event.target instanceof HTMLSelectElement) return;
      if (event.key === "ArrowLeft") void goToPage(page - 1);
      else if (event.key === "ArrowRight") void goToPage(page + 1);
      else if (event.key === " ") {
        // The arrows are already the page, so space is the key left over — and
        // it is what every reader uses (TTS-34). Default prevented so it does
        // not scroll the panel out from under the reading.
        event.preventDefault();
        void useReadAloudStore.getState().toggle();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [bookId, page, goToPage]);

  if (!bookId) {
    return (
      <div className="flex flex-1 items-center justify-center bg-[var(--bg-app)] text-sm text-[var(--text-secondary)]">
        {t("reader.noBookOpen")}
      </div>
    );
  }

  // The index is the same in every language — the pagination comes from
  // `original/` — so switching keeps the current page for free (READ-28).
  const showingOriginalInstead = language !== null && pageLanguage !== language;

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-[var(--bg-app)] text-[var(--text-primary)]">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] px-6 py-4">
        <select
          value={language ?? ORIGINAL}
          onChange={(e) => void setLanguage(e.target.value === ORIGINAL ? null : e.target.value)}
          className="rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-2 py-1 text-sm"
          title={t("reader.language")}
        >
          {languages.map((lang) => (
            <option key={lang.language} value={lang.language}>
              {lang.language} ({lang.pages})
            </option>
          ))}
        </select>

        <div className="ml-auto flex items-center gap-2">
          {/* TTS-33: onde a mão já está, ao lado das setas de página. */}
          <button
            onClick={() => void aloud.toggle()}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            title={aloud.status === "playing" ? t("reader.pauseAloud") : t("reader.readAloud")}
            aria-label={aloud.status === "playing" ? t("reader.pauseAloud") : t("reader.readAloud")}
          >
            {aloud.status === "playing" ? <Pause size={18} /> : <Play size={18} />}
          </button>
          {aloud.status !== "idle" && (
            <button
              onClick={() => aloud.stop()}
              className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
              title={t("reader.stopAloud")}
              aria-label={t("reader.stopAloud")}
            >
              <Square size={18} />
            </button>
          )}
          <button
            onClick={() => void goToPage(page - 1)}
            disabled={page <= 0 || isLoading}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            title={t("reader.previousPage")}
          >
            <ChevronLeft size={18} />
          </button>
          <span className="text-xs text-[var(--text-secondary)]">
            {/* Base 0 on the boundary, base 1 on screen: the shift happens here
                and nowhere else. */}
            {t("reader.pageOf", { page: page + 1, count: pageCount })}
          </span>
          <button
            onClick={() => void goToPage(page + 1)}
            disabled={page >= pageCount - 1 || isLoading}
            className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            title={t("reader.nextPage")}
          >
            <ChevronRight size={18} />
          </button>
        </div>
      </div>

      {/* Silence here would hand the user the original and let them believe it
          is the translation they asked for (READ-12). */}
      {showingOriginalInstead && (
        <p className="flex items-center gap-1.5 border-b border-[var(--border-color)] px-6 py-2 text-xs text-amber-500">
          <AlertTriangle size={14} />
          {t("reader.showingOriginal", { language })}
        </p>
      )}

      {/* Dizer que a voz é a do sistema, e não a escolhida, é o mesmo princípio
          da READ-12: o silêncio faria o usuário acreditar que ouviu a voz que
          pediu (TTS-35). */}
      {aloud.usingSystemVoice && aloud.status !== "idle" && (
        <p className="px-6 py-2 text-xs text-[var(--text-secondary)]">{t("reader.systemVoice")}</p>
      )}
      {aloud.error && <p className="px-6 py-2 text-xs text-red-500">{aloud.error}</p>}
      {error && <p className="px-6 py-2 text-xs text-red-500">{error}</p>}

      {/* FID-02/FID-04. `sandbox` with no token at all is the strictest
          setting there is: the book's own `<script>` cannot run, its CSS
          cannot reach the app's interface, and the app's CSS cannot deform the
          book. Sanitizing the markup by hand would cover only the first of the
          three and would need maintaining against every new trick. The images
          are already `data:` URIs inside `srcdoc` — a sandbox without
          `allow-same-origin` has an opaque origin and could not read a
          `blob:`. */}
      {pageFormat === "html" ? (
        <iframe
          key={`${bookId}-${page}-${pageLanguage}`}
          data-book-page
          srcDoc={srcDoc}
          title={t("reader.bookPage")}
          // `allow-scripts` without `allow-same-origin`: the app's marking
          // script runs, the origin stays opaque, and the book's own JavaScript
          // was already removed at extraction (`reader/sanitize.rs`). Dropping
          // either half of that sentence turns this into a regression (TTS-12).
          sandbox="allow-scripts"
          className="flex-1 w-full border-0 bg-white"
        />
      ) : (
      <div className="flex-1 overflow-y-auto px-6 py-8">
        <div className="flex w-full flex-col gap-4 text-[15px] leading-7">
          {blocks.map((block, i) =>
            block.image ? (
              <Illustration key={`${block.image}-${i}`} bookId={bookId} name={block.image} />
            ) : (
              <p key={i} className="whitespace-pre-wrap">
                {block.text}
              </p>
            ),
          )}
        </div>
      </div>
      )}
    </div>
  );
}
