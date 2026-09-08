// SPEC: read-aloud (TTS-20, TTS-21, TTS-22, TTS-23, TTS-27, TTS-28, TTS-31, TTS-32)

import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, Download, Play, RefreshCw, Trash2, X } from "lucide-react";
import { ttsApi } from "../../lib/ttsApi";
import { useReaderStore } from "../../store/readerStore";
import type { PullProgress, VoiceInfo } from "../../types";

/** A voice download is one file of tens of megabytes; a percentage is the only
 *  honest thing to show while it runs. Same event shape the model download
 *  already uses. */
interface VoiceProgress {
  voice_id: string;
  progress: PullProgress;
}

function megabytes(bytes: number) {
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

export function VoicesList() {
  const { t } = useTranslation();
  const [voices, setVoices] = useState<VoiceInfo[]>([]);
  const [chosen, setChosen] = useState<Record<string, string>>({});
  const [speed, setSpeed] = useState(1);
  const [busy, setBusy] = useState<Record<string, number>>({});
  const [filter, setFilter] = useState("");
  const [language, setLanguage] = useState("");
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    ttsApi.listVoices().then(setVoices).catch((e) => setError(String(e)));
    ttsApi
      .ttsSettings()
      .then((s) => {
        setChosen(s.voices);
        setSpeed(s.speed);
      })
      .catch(() => {});
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    const stop = listen<VoiceProgress>("voice-download-progress", (event) => {
      const { voice_id, progress } = event.payload;
      if (progress.status === "success") {
        setBusy((b) => {
          const next = { ...b };
          delete next[voice_id];
          return next;
        });
        load();
        return;
      }
      if (progress.status === "error") {
        setBusy((b) => {
          const next = { ...b };
          delete next[voice_id];
          return next;
        });
        setError(progress.message ?? String(progress.status));
        return;
      }
      const done = progress.downloaded_bytes ?? 0;
      const total = progress.total_bytes ?? 0;
      setBusy((b) => ({ ...b, [voice_id]: total > 0 ? (done / total) * 100 : 0 }));
    });
    return () => {
      void stop.then((off) => off());
    };
  }, [load]);

  async function download(voice: VoiceInfo) {
    setError(null);
    setBusy((b) => ({ ...b, [voice.id]: 0 }));
    try {
      await ttsApi.downloadVoice(voice.id);
    } catch (e) {
      setError(String(e));
    } finally {
      load();
    }
  }

  /// Unpinning a language falls the reader back to whatever voice matches it,
  /// which is what happens for a language that was never pinned at all.
  async function unpin(language: string) {
    await ttsApi.setTtsVoice(language, null).catch((e) => setError(String(e)));
    setChosen((c) => {
      const next = { ...c };
      delete next[language];
      return next;
    });
  }

  async function choose(voice: VoiceInfo) {
    await ttsApi.setTtsVoice(voice.language, voice.id).catch((e) => setError(String(e)));
    setChosen((c) => ({ ...c, [voice.language]: voice.id }));
  }

  async function remove(voice: VoiceInfo) {
    // Same `window.confirm` the Library uses to drop a language: this base has
    // no modal system and does not need one for a question with two words.
    if (!window.confirm(t("voices.removeConfirm", { name: voice.display_name }))) return;
    const freed = await ttsApi.removeVoice(voice.id).catch((e) => {
      setError(String(e));
      return 0;
    });
    if (freed) setError(t("voices.removed", { size: megabytes(freed) }));
    load();
  }

  /** TTS-28: the sample is the first sentence of the page being read, so the
   *  voice is judged on the book, not on a slogan. */
  async function test(voice: VoiceInfo) {
    setError(null);
    try {
      const page = useReaderStore.getState().text;
      const sentences = page ? await ttsApi.pageUtterances(page) : [];
      const sample = sentences[0]?.text ?? t("voices.sample");
      const bytes = await ttsApi.speakSentence(voice.id, sample);
      const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
      const audio = new Audio(url);
      audio.onended = () => URL.revokeObjectURL(url);
      await audio.play();
    } catch (e) {
      setError(String(e));
    }
  }

  /** Six built-in voices become 176 in 57 languages. Only on demand: the app
   *  is local-first, and a list that phoned home on every open would break
   *  that for nothing. */
  async function refresh() {
    setError(null);
    try {
      const count = await ttsApi.refreshVoiceCatalog();
      setError(t("voices.refreshed", { count }));
      load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function changeSpeed(value: number) {
    setSpeed(value);
    await ttsApi.setTtsSpeed(value).catch((e) => setError(String(e)));
  }

  // The languages actually present in the catalog, named as the catalog names
  // them. Derived instead of hard-coded because the list goes from 6 voices to
  // 176 in 57 languages the moment someone loads the full catalog.
  const languages = Array.from(
    new Map(voices.map((v) => [v.language, v.language_name])).entries(),
  ).sort((a, b) => a[1].localeCompare(b[1]));

  // A list of 176 is only useful with a way through it. The dropdown narrows to
  // one language; the text box still matches the language name as well as the
  // code, which is what makes "português" find `pt_BR`.
  const needle = filter.trim().toLowerCase();
  const shown = voices.filter((v) => {
    if (language && v.language !== language) return false;
    if (!needle) return true;
    return `${v.display_name} ${v.language} ${v.language_name} ${v.quality}`
      .toLowerCase()
      .includes(needle);
  });

  const pinned = Object.entries(chosen);

  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      {/* One voice per language is what the reader actually resolves, so the
          pins belong on screen: without this the only way to know which voice
          answers for which language was to hit play and listen. */}
      {pinned.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-[var(--text-secondary)]">{t("voices.chosenLabel")}</span>
          {pinned.map(([language, id]) => {
            const voice = voices.find((v) => v.id === id);
            return (
              <span
                key={language}
                className="flex items-center gap-1.5 rounded-full border border-[var(--border-color)] px-2 py-0.5 text-xs"
              >
                {voice?.language_name ?? language} · {voice?.display_name ?? id}
                <button
                  onClick={() => void unpin(language)}
                  title={t("voices.unpin")}
                  aria-label={t("voices.unpin")}
                  className="text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                >
                  <X size={12} />
                </button>
              </span>
            );
          })}
        </div>
      )}

      <div className="flex items-center gap-3">
        <label htmlFor="tts-speed" className="text-sm">
          {t("voices.speed")}
        </label>
        <input
          id="tts-speed"
          type="range"
          min={0.5}
          max={2}
          step={0.1}
          value={speed}
          onChange={(e) => void changeSpeed(Number(e.target.value))}
          className="w-48"
        />
        <span className="w-12 text-sm text-[var(--text-secondary)]">{speed.toFixed(1)}×</span>
      </div>

      <div className="flex items-center gap-3">
        <select
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          aria-label={t("voices.languageFilter")}
          className="rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-2 py-1 text-sm"
        >
          <option value="">{t("voices.allLanguages")}</option>
          {languages.map(([code, name]) => (
            <option key={code} value={code}>
              {name}
            </option>
          ))}
        </select>
        <input
          type="search"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t("voices.filter")}
          className="flex-1 rounded-md border border-[var(--border-color)] bg-[var(--bg-app)] px-2 py-1 text-sm"
        />
        <button
          onClick={() => void refresh()}
          className="flex items-center gap-1.5 rounded-md border border-[var(--border-color)] px-2 py-1 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          <RefreshCw size={14} />
          {t("voices.refresh")}
        </button>
      </div>

      {error && <p className="text-xs text-[var(--text-secondary)]">{error}</p>}

      {shown.some((v) => v.engine === "kokoro") && (
        <p className="text-xs text-[var(--text-secondary)]">{t("voices.sharedModel")}</p>
      )}

      {shown.length === 0 && (
        <p className="text-xs text-[var(--text-secondary)]">{t("voices.empty")}</p>
      )}

      <ul className="flex flex-col gap-2">
        {shown.map((voice) => {
          const percent = busy[voice.id];
          const isChosen = chosen[voice.language] === voice.id;
          // Every Kokoro voice is a 0,5 MB style vector inside one 325 MB model
          // they all share. Showing the full size on the tenth row, and a trash
          // can the backend refuses, is what made one download look like ten.
          const shared = voice.engine === "kokoro";
          return (
            <li
              key={voice.id}
              className="flex items-center gap-3 rounded-md border border-[var(--border-color)] px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <p className="text-sm">
                  {voice.display_name}{" "}
                  <span className="text-[var(--text-secondary)]">
                    · {voice.language_name} · {voice.engine}
                  </span>
                </p>
                <p className="text-xs text-[var(--text-secondary)]">
                  {shared && voice.installed
                    ? t("voices.included")
                    : megabytes(voice.download_bytes)}
                  {percent !== undefined && ` · ${percent.toFixed(0)}%`}
                </p>
              </div>

              {voice.installed ? (
                <>
                  <button
                    onClick={() => void test(voice)}
                    title={t("voices.test")}
                    aria-label={t("voices.test")}
                    className="rounded-md p-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                  >
                    <Play size={16} />
                  </button>
                  <button
                    onClick={() => void choose(voice)}
                    disabled={isChosen}
                    className={`rounded-md px-2 py-1 text-xs ${
                      isChosen
                        ? "text-[var(--accent)]"
                        : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                    }`}
                  >
                    {isChosen ? <Check size={16} /> : t("voices.use")}
                  </button>
                  {!shared && (
                    <button
                      onClick={() => void remove(voice)}
                      title={t("voices.remove")}
                      aria-label={t("voices.remove")}
                      className="rounded-md p-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                </>
              ) : (
                <button
                  onClick={() => void download(voice)}
                  disabled={percent !== undefined}
                  className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-40"
                >
                  <Download size={16} />
                  {percent !== undefined ? `${percent.toFixed(0)}%` : t("voices.download")}
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
