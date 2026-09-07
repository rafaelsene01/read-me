// SPEC: book-reader (READ-06, READ-07, READ-21)

import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useRuntimeStore } from "../../store/runtimeStore";
import { ModelDownloadCard } from "../Runtime/ModelDownloadCard";
import type { BookRecord } from "../../types";

// `null` is "do not translate" on the whole boundary, but a radio value is
// always a string — this is where the two representations meet.
const ORIGINAL = "original";
// The two languages the app itself ships in `src/i18n/locales/`. Offering more
// would promise quality nobody measured, in any of them.
export const TRANSLATION_LANGUAGES = ["pt", "en"];
const LANGUAGE_LABEL_KEY: Record<string, string> = {
  original: "library.dialogOriginal",
  pt: "library.dialogPt",
  en: "library.dialogEn",
};

// Mirrors `DEFAULT_TRANSLATION_MODEL` in src-tauri/src/reader/translate.rs.
// Hand-copied and ungated (AD-054): renaming the catalog id there leaves this
// string pointing at nothing while `cargo check` and `npm run build` stay clean.
export const DEFAULT_TRANSLATION_MODEL = "gguf-qwen2.5-7b";

// Measured 2026-09-06 (T1) against a real llama-server: Qwen2.5 7B Q4_K_M
// translated a 2,161-character page of Moby-Dick in 12.53 s, one request per
// paragraph (3 of them) → ~63 min for 300 pages. It is an extrapolation from a
// single page — a page with 10 short paragraphs pays 10× the 174 ms fixed cost
// — so the screen says "about", and says where the number came from.
const SECONDS_PER_TRANSLATED_PAGE = 12.5;

interface Props {
  book: BookRecord;
  onClose: () => void;
  onConfirm: (language: string | null) => void;
}

export function ProcessDialog({ book, onClose, onConfirm }: Props) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDialogElement>(null);
  // READ-06: "do not translate" is the pre-selected option, because processing
  // without translation is seconds and no LLM — the path most books take.
  const [choice, setChoice] = useState(ORIGINAL);
  const {
    activeModel,
    downloadableModels,
    downloadProgress,
    loadActiveModel,
    loadDownloadableModels,
    downloadModel,
  } = useRuntimeStore();

  useEffect(() => {
    // `showModal()` is what gives Escape-to-close and the focus trap for free;
    // rendering `<dialog open>` gives neither.
    dialogRef.current?.showModal();
  }, []);

  useEffect(() => {
    loadActiveModel();
    loadDownloadableModels();
  }, [loadActiveModel, loadDownloadableModels]);

  const language = choice === ORIGINAL ? null : choice;
  // READ-21: the model is only named when a translation is actually asked for.
  const needsModel = language !== null && activeModel === null;
  const defaultModel = downloadableModels.find((m) => m.id === DEFAULT_TRANSLATION_MODEL);
  const minutes = Math.max(1, Math.round((book.page_count * SECONDS_PER_TRANSLATED_PAGE) / 60));

  return (
    <dialog
      ref={dialogRef}
      onClose={onClose}
      className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-app)] p-0 text-[var(--text-primary)] backdrop:bg-black/50"
    >
      <div className="w-[26rem] max-w-[90vw] p-5">
        <h2 className="truncate text-sm font-semibold" title={book.filename}>
          {t("library.dialogTitle", { name: book.filename })}
        </h2>

        <fieldset className="mt-4 border-0 p-0">
          <legend className="text-xs text-[var(--text-secondary)]">
            {t("library.dialogLanguage")}
          </legend>
          <div className="mt-2 space-y-1.5">
            {[ORIGINAL, ...TRANSLATION_LANGUAGES].map((value) => (
              <label key={value} className="flex items-center gap-2 text-sm">
                <input
                  type="radio"
                  name="reading-language"
                  value={value}
                  checked={choice === value}
                  onChange={() => setChoice(value)}
                />
                {t(LANGUAGE_LABEL_KEY[value])}
              </label>
            ))}
          </div>
        </fieldset>

        {/* READ-07: shown before anything starts, and only when translating. */}
        {language !== null && (
          <div className="mt-4 rounded-md border border-[var(--border-color)] px-3 py-2">
            <p className="text-xs">
              {book.page_count > 0
                ? t("library.dialogEstimate", { minutes, pages: book.page_count })
                : t("library.dialogEstimateUnknown")}
            </p>
            <p className="mt-1 text-xs text-[var(--text-secondary)]">
              {t("library.dialogEstimateNote")}
            </p>
          </div>
        )}

        {needsModel && (
          <div className="mt-3 space-y-2">
            <p className="text-xs text-amber-500">
              {t("library.dialogNoActiveModel", { model: DEFAULT_TRANSLATION_MODEL })}
            </p>
            {/* Zero new download code: the curated entry, the progress bar and
                the URL validation are the ones the Runtime screen already uses. */}
            {defaultModel && (
              <ModelDownloadCard
                model={defaultModel}
                progress={downloadProgress[defaultModel.pull_identifier]}
                onDownload={() => downloadModel(defaultModel.pull_identifier)}
              />
            )}
          </div>
        )}

        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md border border-[var(--border-color)] px-3 py-1.5 text-sm hover:bg-[var(--bg-elevated)]"
          >
            {t("library.dialogCancel")}
          </button>
          <button
            onClick={() => onConfirm(language)}
            // Disabled, not hidden: without an active model the backend refuses
            // and nothing is deleted, and a button that only ever errors is
            // worse than one that says why it cannot be pressed.
            disabled={needsModel}
            className="rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)] disabled:opacity-50"
          >
            {t("library.dialogStart")}
          </button>
        </div>
      </div>
    </dialog>
  );
}
