// SPEC: book-reader (READ-12, READ-15, READ-16, READ-17, READ-28, READ-33),
//       book-illustrations (ILLUS-06), epub-fidelity (FID-02, FID-04, FID-14),
//       read-aloud (TTS-03, TTS-05, TTS-12, TTS-17, TTS-32, TTS-33, TTS-34),
//       book-reader (READ-34)

import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronLeft, ChevronRight, Gauge, Pause, Play, Square } from "lucide-react";
import { useReaderStore } from "../../store/readerStore";
import { readerApi } from "../../lib/readerApi";
import { READER_SCRIPT, READER_SCRIPT_CSS } from "../../lib/readerScript";
import { useReadAloudStore } from "../../store/readAloudStore";
import { useConfigStore } from "../../store/configStore";
import { pageColors } from "../../lib/theme";
import type { BookLanguage } from "../../types";

// `null` is "read the original" on the whole boundary, but a <select> value is
// always a string — this is the only place the two representations meet.
const ORIGINAL = "original";

// READ-34: text size, in percent of the book's own. A reading preference of
// this window, like the Library's view, so it lives in its storage and a
// storage that throws just means 100%.
const FONT_KEY = "readme-reader-font-scale";
const FONT_MIN = 70;
const FONT_MAX = 200;
const FONT_STEP = 10;

function storedFontScale(): number {
  try {
    const value = Number(localStorage.getItem(FONT_KEY));
    return value >= FONT_MIN && value <= FONT_MAX ? value : 100;
  } catch {
    return 100;
  }
}

// The marker the extractors leave in the text, as a whole paragraph
// (ILLUS-04). The shape is duplicated from Rust on purpose: it crosses the
// boundary as plain text, so there is nothing generated to share, and the
// definition lives in `reader/illustrations.rs`.
// Four to six digits, like `is_image_name`: a book past 9999 images names the
// 10,000th `10000.png` (AD-077).
const IMAGE_MARKER = /^\[\[image: (\d{4,6}\.[a-z0-9]{1,5})\]\]$/;

/** Bumped for every document the page frame shows; see `docKey` below. */
let documentCounter = 0;

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
  const [fontScale, setFontScale] = useState(storedFontScale);

  function changeFont(delta: number) {
    const next = Math.min(FONT_MAX, Math.max(FONT_MIN, fontScale + delta));
    setFontScale(next);
    try {
      localStorage.setItem(FONT_KEY, String(next));
    } catch {
      // Not remembering the size is not worth a banner.
    }
  }
  const blocks = useMemo(() => blocksOf(text), [text]);
  const aloud = useReadAloudStore();
  // Any change of theme or custom color rebuilds the page with the new colors.
  const themeKey = useConfigStore((s) => JSON.stringify([s.config?.theme, s.config?.custom_theme]));

  // The app's own script goes in just before `</body>`, so it lives in TS
  // where it is readable, and the document the backend assembled stays the
  // book's (FID-02). The page is already sanitized on disk, which is what
  // makes `allow-scripts` safe to hand it.
  const srcDoc = useMemo(() => {
    if (pageFormat !== "html") return "";
    // FID-14: the theme wins over the book's own colors - background and text,
    // like a dedicated e-reader - while the rest of its CSS (italics, fonts,
    // alignment) stays. `!important` because the book's stylesheet comes after
    // the base one and would otherwise win. The read-aloud fallback span is
    // excluded, or the transparent-background rule would erase the mark.
    // `pageColors` only hands back validated hex, so nothing else gets in here.
    const colors = pageColors();
    const themeCss = colors
      ? `html,body{background:${colors.background} !important;color:${colors.text} !important}` +
        `body *:not(.readaloud-mark){color:inherit !important;background-color:transparent !important}`
      : "";
    // READ-34. `zoom` and not `font-size`: a book that sets its sizes in `px`
    // ignores a root font size, and zoom scales whatever unit it used - text
    // reflows to the frame's width, and images stay inside `max-width: 100%`.
    const zoomCss = fontScale !== 100 ? `body{zoom:${fontScale / 100}}` : "";
    const injected = `<style>${themeCss}${zoomCss}${READER_SCRIPT_CSS}</style><script>${READER_SCRIPT}</script>`;
    return text.includes("</body>")
      ? text.replace("</body>", `${injected}</body>`)
      : text + injected;
    // `themeKey` is read only as a dependency: `pageColors` reads the CSS
    // variables that `applyTheme` already wrote by the time the config changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, pageFormat, themeKey, fontScale]);

  // A new iframe element for every document, never a `srcdoc` swapped on a
  // live one (READ-15, AD-075). The old key was book + page + language, and
  // reopening the same reading - back from the Library, or "Ler" on the book
  // already on that page - keeps all three: `openBook` clears the text and
  // loads the same page, so the SAME element went page -> empty document ->
  // page, and the user reported it staying blank until a page turn changed the
  // key and remounted the frame. Keying by document takes that path away.
  const docKey = useMemo(() => {
    documentCounter += 1;
    return documentCounter;
  }, [srcDoc]);

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
    void useReadAloudStore.getState().loadSpeed();
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
      // The language <select> and the speed slider use the arrows to change
      // their value: stealing them there would move the page instead.
      if (event.target instanceof HTMLSelectElement || event.target instanceof HTMLInputElement) {
        return;
      }
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
        {/* READ-33: with only the original there is nothing to choose. */}
        {languages.length > 1 && (
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
        )}

        <div className="ml-auto flex items-center gap-2">
          {/* READ-34: the small A shrinks, the big A grows. */}
          <div className="flex items-center text-[var(--text-secondary)]">
            <button
              onClick={() => changeFont(-FONT_STEP)}
              disabled={fontScale <= FONT_MIN}
              title={t("reader.fontSmaller")}
              aria-label={t("reader.fontSmaller")}
              className="rounded-md px-1.5 py-1 text-xs hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            >
              A
            </button>
            <span className="w-10 text-center text-xs">{fontScale}%</span>
            <button
              onClick={() => changeFont(FONT_STEP)}
              disabled={fontScale >= FONT_MAX}
              title={t("reader.fontLarger")}
              aria-label={t("reader.fontLarger")}
              className="rounded-md px-1.5 py-0.5 text-base hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            >
              A
            </button>
          </div>
          {/* TTS-32 in the reader: the same stored speed Settings edits. */}
          <label
            className="flex items-center gap-1.5 text-xs text-[var(--text-secondary)]"
            title={t("voices.speed")}
          >
            <Gauge size={16} />
            <input
              type="range"
              min={0.5}
              max={2}
              step={0.1}
              value={aloud.speed}
              onChange={(e) => void aloud.setSpeed(Number(e.target.value))}
              aria-label={t("voices.speed")}
              className="w-20"
            />
            <span className="w-8">{aloud.speed.toFixed(1)}×</span>
          </label>
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
      {pageFormat === "html" && !text ? (
        // The page is on its way (`openBook` clears the text first). No frame
        // at all until it arrives, so the one that renders holds the page.
        <div className="flex-1 bg-[var(--bg-app)]" />
      ) : pageFormat === "html" ? (
        <iframe
          key={docKey}
          data-book-page
          srcDoc={srcDoc}
          title={t("reader.bookPage")}
          // `allow-scripts` without `allow-same-origin`: the app's marking
          // script runs, the origin stays opaque, and the book's own JavaScript
          // was already removed at extraction (`reader/sanitize.rs`). Dropping
          // either half of that sentence turns this into a regression (TTS-12).
          sandbox="allow-scripts"
          className="flex-1 w-full border-0 bg-[var(--bg-app)]"
        />
      ) : (
      <div className="flex-1 overflow-y-auto px-6 py-8">
        {/* READ-34 on the plain-text path: the size is the one number, and the
            line height is relative so a larger font does not overlap. */}
        <div
          className="flex w-full flex-col gap-4 leading-[1.75]"
          style={{ fontSize: `${(15 * fontScale) / 100}px` }}
        >
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
