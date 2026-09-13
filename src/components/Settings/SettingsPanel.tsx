// SPEC: app-shell (SHELL-09), settings-storage-i18n (CFG-05, CFG-09),
//       self-contained-runtime (SELF-01), read-aloud (TTS-20, TTS-22, TTS-32)

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, FolderOpen, RefreshCw } from "lucide-react";
import { useConfigStore } from "../../store/configStore";
import { useUiStore } from "../../store/uiStore";
import { useUpdateStore } from "../../store/updateStore";
import { configApi } from "../../lib/configApi";
import { SUPPORTED_THEMES, THEME_LABEL_KEYS, applyTheme, currentColors } from "../../lib/theme";
import type { CustomTheme } from "../../types";
import { SUPPORTED_LANGUAGES, type SupportedLanguage } from "../../i18n";
import { RuntimeCard } from "../Runtime/RuntimeCard";
import { ModelsList } from "../Runtime/ModelsList";
import { VoicesList } from "../Runtime/VoicesList";

const LANGUAGE_LABEL_KEYS: Record<SupportedLanguage, string> = {
  en: "settings.languageEnglish",
  pt: "settings.languagePortuguese",
};

// Runtime, models and voices used to be a screen of their own behind a
// sidebar entry; they are settings like the rest, so they became tabs here
// (SHELL-09). Voices stay next to models because a voice **is** a model: same
// catalog shape, same download, same folder next to the GGUF.
const TABS = [
  ["general", "settings.tabGeneral"],
  ["storage", "settings.storageFolder"],
  ["updates", "settings.updates"],
  ["runtime", "runtime.tabRuntime"],
  ["models", "runtime.tabModels"],
  ["voices", "runtime.tabVoices"],
] as const;
type Tab = (typeof TABS)[number][0];

export function SettingsPanel() {
  const { t } = useTranslation();
  const { config, setTheme, setCustomTheme, setLanguage, setBasePath } = useConfigStore();
  const setActiveView = useUiStore((s) => s.setActiveView);
  const [isChangingFolder, setIsChangingFolder] = useState(false);
  const [tab, setTab] = useState<Tab>("general");
  /** The colors being picked. Painted on every change, persisted only once the
   *  picker settles: a native color input fires on every pixel of a drag. */
  const [draft, setDraft] = useState<CustomTheme | null>(null);

  const {
    settings: updateSettings,
    available,
    checking,
    checkedManually,
    error: updateError,
    init: initUpdates,
    checkNow,
    setAutoCheck,
  } = useUpdateStore();

  // The panel can be opened before the boot effect has loaded settings.
  useEffect(() => {
    if (!updateSettings) void initUpdates();
  }, [updateSettings, initUpdates]);

  useEffect(() => {
    setDraft(config?.custom_theme ?? null);
  }, [config?.custom_theme]);

  useEffect(() => {
    const saved = config?.custom_theme;
    if (!draft || config?.theme !== "custom") return;
    if (
      saved &&
      saved.background === draft.background &&
      saved.text === draft.text &&
      saved.accent === draft.accent
    ) {
      return;
    }
    const timer = window.setTimeout(() => void setCustomTheme(draft), 400);
    return () => window.clearTimeout(timer);
  }, [draft, config?.theme, config?.custom_theme, setCustomTheme]);

  if (!config) return null;

  async function handleChangeFolder() {
    const picked = await configApi.pickFolder();
    if (!picked || picked === config?.base_path) return;
    setIsChangingFolder(true);
    try {
      await setBasePath(picked);
    } finally {
      setIsChangingFolder(false);
    }
  }

  // Choosing "custom" starts from the saved colors or, the first time, from the
  // theme on screen - so the app does not jump to white on the click.
  function chooseCustom() {
    const colors = config?.custom_theme ?? currentColors();
    if (colors) void setCustomTheme(colors);
  }

  function changeCustom(key: keyof CustomTheme, value: string) {
    if (!draft) return;
    const next = { ...draft, [key]: value };
    setDraft(next);
    applyTheme("custom", next);
  }

  return (
    <div className="flex flex-1 flex-col overflow-y-auto bg-[var(--bg-app)] text-[var(--text-primary)]">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] px-6 py-4">
        <button
          onClick={() => setActiveView("reader")}
          className="rounded-md p-1.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
          title={t("settings.back")}
        >
          <ArrowLeft size={18} />
        </button>
        <h1 className="text-base font-semibold">{t("settings.title")}</h1>
      </div>

      <div className="flex flex-wrap gap-1 border-b border-[var(--border-color)] px-6 pt-3">
        {TABS.map(([tabOption, labelKey]) => (
          <button
            key={tabOption}
            onClick={() => setTab(tabOption)}
            className={`rounded-t-md px-3 py-2 text-sm font-medium ${
              tab === tabOption
                ? "border-b-2 border-[var(--accent)] text-[var(--text-primary)]"
                : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            }`}
          >
            {t(labelKey)}
          </button>
        ))}
      </div>

      <div className="mx-auto w-full max-w-2xl space-y-8 px-6 py-8">
        {tab === "general" && (
          <>
            <section>
              <h2 className="text-sm font-medium">{t("settings.theme")}</h2>
              <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
                {SUPPORTED_THEMES.map((themeOption) => (
                  <button
                    key={themeOption}
                    onClick={() =>
                      themeOption === "custom" ? chooseCustom() : void setTheme(themeOption)
                    }
                    className={`rounded-md border px-3 py-2 text-sm ${
                      config.theme === themeOption
                        ? "border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-fg)]"
                        : "border-[var(--border-color)] hover:bg-[var(--bg-elevated)]"
                    }`}
                  >
                    {t(THEME_LABEL_KEYS[themeOption])}
                  </button>
                ))}
              </div>

              {/* CFG-09: native color inputs - the platform already has a picker. */}
              {config.theme === "custom" && draft && (
                <div className="mt-3 flex flex-wrap gap-4">
                  {(["background", "text", "accent"] as const).map((key) => (
                    <label key={key} className="flex items-center gap-2 text-sm">
                      <input
                        type="color"
                        value={draft[key]}
                        onChange={(e) => changeCustom(key, e.target.value)}
                        className="h-8 w-10 cursor-pointer rounded border border-[var(--border-color)] bg-transparent"
                      />
                      {t(`settings.custom${key[0].toUpperCase()}${key.slice(1)}`)}
                    </label>
                  ))}
                </div>
              )}
            </section>

            <section>
              <h2 className="text-sm font-medium">{t("settings.language")}</h2>
              <div className="mt-2 grid grid-cols-2 gap-2 sm:w-64">
                {SUPPORTED_LANGUAGES.map((langOption) => (
                  <button
                    key={langOption}
                    onClick={() => setLanguage(langOption)}
                    className={`rounded-md border px-3 py-2 text-sm ${
                      config.language === langOption
                        ? "border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-fg)]"
                        : "border-[var(--border-color)] hover:bg-[var(--bg-elevated)]"
                    }`}
                  >
                    {t(LANGUAGE_LABEL_KEYS[langOption])}
                  </button>
                ))}
              </div>
            </section>
          </>
        )}

        {tab === "storage" && (
          <section>
            <h2 className="text-sm font-medium">{t("settings.storageFolder")}</h2>
            <p className="mt-2 truncate rounded-md border border-[var(--border-color)] bg-[var(--bg-elevated)] px-3 py-2 text-sm text-[var(--text-secondary)]" title={config.base_path}>
              {config.base_path}
            </p>
            <button
              onClick={handleChangeFolder}
              disabled={isChangingFolder}
              className="mt-2 flex items-center gap-1.5 rounded-md bg-[var(--accent)] px-3 py-1.5 text-sm font-medium text-[var(--accent-fg)] hover:bg-[var(--accent-hover)] disabled:opacity-50"
            >
              <FolderOpen size={14} />
              {t("settings.changeFolder")}
            </button>
          </section>
        )}

        {tab === "updates" && (
          <section>
            <h2 className="text-sm font-medium">{t("settings.updates")}</h2>

            <div className="mt-2 flex items-center gap-2 rounded-md border border-[var(--border-color)] bg-[var(--bg-elevated)] px-3 py-2 text-sm">
              <span className="text-[var(--text-secondary)]">
                {t("settings.currentVersion", {
                  version: updateSettings?.current_version ?? "—",
                })}
              </span>
              {updateSettings && (
                <span className="rounded-full border border-[var(--border-color)] px-2 py-0.5 text-xs text-[var(--text-secondary)]">
                  {t(
                    updateSettings.flavor === "portable"
                      ? "settings.flavorPortable"
                      : "settings.flavorInstalled",
                  )}
                </span>
              )}
            </div>

            <label className="mt-3 flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={updateSettings?.auto_check ?? true}
                onChange={(e) => setAutoCheck(e.target.checked)}
              />
              <span>
                {t("settings.autoUpdateCheck")}
                <span className="block text-xs text-[var(--text-secondary)]">
                  {t("settings.autoUpdateCheckHint")}
                </span>
              </span>
            </label>

            <button
              onClick={checkNow}
              disabled={checking}
              className="mt-3 flex items-center gap-1.5 rounded-md border border-[var(--border-color)] px-3 py-1.5 text-sm hover:bg-[var(--bg-elevated)] disabled:opacity-50"
            >
              <RefreshCw size={14} className={checking ? "animate-spin" : undefined} />
              {t("settings.checkForUpdates")}
            </button>

            {/* Unlike the boot check, a manual check always says something —
                "you are up to date" is a real answer the user asked for. */}
            {updateError ? (
              <p className="mt-2 text-xs text-[var(--danger,#f87171)]">{updateError}</p>
            ) : checkedManually ? (
              <p className="mt-2 text-xs text-[var(--text-secondary)]">
                {available
                  ? t("settings.updateFound", { version: available.version })
                  : t("settings.upToDate")}
              </p>
            ) : null}
          </section>
        )}

        {tab === "runtime" && <RuntimeCard />}
        {tab === "models" && <ModelsList />}
        {tab === "voices" && <VoicesList />}
      </div>
    </div>
  );
}
