// SPEC: book-reader (READ-24, READ-25, READ-26, READ-27, READ-29, READ-30)

import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Download, Loader2, Trash2, X } from "lucide-react";
import { useLibraryStore } from "../../store/libraryStore";
import { useRuntimeStore } from "../../store/runtimeStore";
import { readerApi } from "../../lib/readerApi";
import { ModelDownloadCard } from "../Runtime/ModelDownloadCard";
import { DEFAULT_TRANSLATION_MODEL, TRANSLATION_LANGUAGES } from "./ProcessDialog";
import type { BookLanguage, BookRecord } from "../../types";

interface Props {
  book: BookRecord;
  onClose: () => void;
}

export function BookEditPanel({ book, onClose }: Props) {
  const { t } = useTranslation();
  const progress = useLibraryStore((s) => s.progress[book.id]);
  const loadBooks = useLibraryStore((s) => s.loadBooks);
  const {
    activeModel,
    installedModels,
    downloadableModels,
    downloadProgress,
    loadActiveModel,
    loadInstalledModels,
    loadDownloadableModels,
    setActiveModel,
    downloadModel,
  } = useRuntimeStore();

  const [languages, setLanguages] = useState<BookLanguage[]>([]);
  // Base-0 page indexes, the same base the whole readerApi boundary uses.
  const [selected, setSelected] = useState<number[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshLanguages = useCallback(() => {
    readerApi
      .listBookLanguages(book.id)
      .then(setLanguages)
      .catch((err) => setError(String(err)));
  }, [book.id]);

  useEffect(() => {
    refreshLanguages();
    loadActiveModel();
    loadInstalledModels();
    loadDownloadableModels();
  }, [refreshLanguages, loadActiveModel, loadInstalledModels, loadDownloadableModels]);

  // Every mutation here ends the same way: the row and the language counts are
  // both stale until reloaded, and forgetting one of the two is the bug.
  async function run(work: () => Promise<unknown>) {
    setIsRunning(true);
    setError(null);
    try {
      await work();
    } catch (err) {
      setError(String(err));
    }
    setIsRunning(false);
    refreshLanguages();
    await loadBooks();
  }

  const readingLanguage = book.reading_language;
  const missing = TRANSLATION_LANGUAGES.filter(
    (code) => !languages.some((l) => l.language === code),
  );
  const defaultModel = downloadableModels.find((m) => m.id === DEFAULT_TRANSLATION_MODEL);
  const translation =
    progress && progress.language !== null && progress.total > 0 ? progress : null;

  function togglePage(page: number) {
    setSelected((s) => (s.includes(page) ? s.filter((p) => p !== page) : [...s, page]));
  }

  function removeLanguage(lang: BookLanguage) {
    // READ-29: the count about to be lost belongs in the question itself —
    // "remove pt?" does not tell anyone that 300 translated pages go with it.
    const message = t("library.editRemoveConfirm", {
      language: lang.language,
      pages: lang.pages,
    });
    if (!window.confirm(message)) return;
    void run(() => readerApi.removeLanguage(book.id, lang.language));
  }

  return (
    <div className="mt-2 space-y-4 rounded-md border border-[var(--border-color)] bg-[var(--bg-elevated)] p-3">
      <div className="flex items-start justify-between gap-2">
        <h3 className="truncate text-sm font-semibold" title={book.filename}>
          {t("library.editTitle", { name: book.filename })}
        </h3>
        <button
          onClick={onClose}
          className="rounded-md p-1 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          title={t("library.editClose")}
        >
          <X size={14} />
        </button>
      </div>

      {error && <p className="text-xs text-red-500">{error}</p>}

      {/* READ-25: the model, and the restart it costs, said before it happens. */}
      <section className="space-y-1.5">
        <p className="text-xs font-medium">{t("library.editModel")}</p>
        {installedModels.length === 0 ? (
          <div className="space-y-2">
            <p className="text-xs text-amber-500">
              {t("library.dialogNoActiveModel", { model: DEFAULT_TRANSLATION_MODEL })}
            </p>
            {defaultModel && (
              <ModelDownloadCard
                model={defaultModel}
                progress={downloadProgress[defaultModel.pull_identifier]}
                onDownload={() => downloadModel(defaultModel.pull_identifier)}
              />
            )}
          </div>
        ) : (
          <>
            <select
              value={activeModel?.name ?? ""}
              disabled={isRunning}
              onChange={(e) => void run(() => setActiveModel(e.target.value))}
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-2 py-1 text-xs"
            >
              <option value="" disabled>
                {t("library.editModelNone")}
              </option>
              {installedModels.map((m) => (
                <option key={m.name} value={m.name}>
                  {m.name}
                </option>
              ))}
            </select>
            <p className="text-xs text-[var(--text-secondary)]">{t("library.editModelRestart")}</p>
          </>
        )}
      </section>

      {/* READ-27 / READ-29 / READ-30: the languages this book has on disk. */}
      <section className="space-y-1.5">
        <p className="text-xs font-medium">{t("library.editLanguages")}</p>
        <ul className="space-y-1">
          <li className="flex items-center justify-between gap-2 text-xs">
            <span>
              {t("library.dialogOriginal")} ·{" "}
              {t("library.editPagesReady", { pages: book.page_count })}
            </span>
            {readingLanguage === null ? (
              <span className="text-[var(--text-secondary)]">{t("library.editReading")}</span>
            ) : (
              <button
                disabled={isRunning}
                // READ-27: switching what is read deletes no translation.
                onClick={() => void run(() => readerApi.setReadingLanguage(book.id, null))}
                className="rounded-md border border-[var(--border-color)] px-2 py-0.5 hover:bg-[var(--bg-app)] disabled:opacity-50"
              >
                {t("library.editSetReading")}
              </button>
            )}
          </li>
          {languages.map((lang) => (
            <li key={lang.language} className="flex items-center justify-between gap-2 text-xs">
              <span>
                {lang.language} · {t("library.editPagesReady", { pages: lang.pages })}
              </span>
              <span className="flex items-center gap-1.5">
                {lang.reading ? (
                  <span className="text-[var(--text-secondary)]">{t("library.editReading")}</span>
                ) : (
                  <button
                    disabled={isRunning}
                    onClick={() =>
                      void run(() => readerApi.setReadingLanguage(book.id, lang.language))
                    }
                    className="rounded-md border border-[var(--border-color)] px-2 py-0.5 hover:bg-[var(--bg-app)] disabled:opacity-50"
                  >
                    {t("library.editSetReading")}
                  </button>
                )}
                <button
                  disabled={isRunning}
                  onClick={() => removeLanguage(lang)}
                  title={t("library.editRemoveLanguage")}
                  className="rounded-md p-1 text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-50"
                >
                  <Trash2 size={13} />
                </button>
              </span>
            </li>
          ))}
        </ul>
        {missing.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {missing.map((code) => (
              <button
                key={code}
                disabled={isRunning}
                onClick={() => void run(() => readerApi.addLanguage(book.id, code))}
                className="flex items-center gap-1 rounded-md border border-[var(--border-color)] px-2 py-0.5 text-xs hover:bg-[var(--bg-app)] disabled:opacity-50"
              >
                <Download size={12} />
                {t("library.editAddLanguage", { language: code })}
              </button>
            ))}
          </div>
        )}
      </section>

      {/* READ-26: one page can be redone without touching page_count. */}
      <section className="space-y-1.5">
        <p className="text-xs font-medium">{t("library.editPages")}</p>
        {readingLanguage === null ? (
          <p className="text-xs text-[var(--text-secondary)]">{t("library.editPagesNoLanguage")}</p>
        ) : (
          <>
            <div className="flex max-h-40 flex-wrap gap-1 overflow-y-auto rounded-md border border-[var(--border-color)] p-1.5">
              {Array.from({ length: book.page_count }, (_, page) => (
                <button
                  key={page}
                  onClick={() => togglePage(page)}
                  className={`w-9 rounded px-1 py-0.5 text-[11px] ${
                    selected.includes(page)
                      ? "bg-[var(--accent)] text-[var(--accent-fg)]"
                      : "hover:bg-[var(--bg-app)]"
                  }`}
                >
                  {page + 1}
                </button>
              ))}
            </div>
            <button
              disabled={isRunning || selected.length === 0}
              onClick={() =>
                void run(async () => {
                  await readerApi.retranslatePages(book.id, readingLanguage, selected);
                  setSelected([]);
                })
              }
              className="rounded-md bg-[var(--accent)] px-2 py-1 text-xs font-medium text-[var(--accent-fg)] disabled:opacity-50"
            >
              {t("library.editRetranslate", { count: selected.length })}
            </button>
          </>
        )}
      </section>

      <div className="flex items-center justify-between gap-2 border-t border-[var(--border-color)] pt-2">
        <button
          disabled={isRunning}
          // The whole book: re-extracts and repaginates (T5), which is why it is
          // a separate button from retranslating a selection.
          onClick={() => void run(() => readerApi.processBook(book.id, readingLanguage))}
          className="rounded-md border border-[var(--border-color)] px-2 py-1 text-xs hover:bg-[var(--bg-app)] disabled:opacity-50"
        >
          {t("library.editReprocess")}
        </button>
        {isRunning && (
          <span className="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
            <Loader2 size={13} className="animate-spin" />
            {translation
              ? t("library.translating", {
                  language: translation.language,
                  done: translation.done,
                  total: translation.total,
                })
              : t("library.processing")}
            <button
              onClick={() => void readerApi.cancelProcessing(book.id)}
              className="rounded-md border border-[var(--border-color)] px-2 py-0.5 hover:bg-[var(--bg-app)]"
            >
              {t("library.cancel")}
            </button>
          </span>
        )}
      </div>
    </div>
  );
}
