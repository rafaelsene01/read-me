// SPEC: settings-storage-i18n (CFG-05, CFG-06, CFG-07, CFG-09)

import { create } from "zustand";
import { configApi } from "../lib/configApi";
import { applyLanguage } from "../i18n";
import { applyTheme, normalizeTheme } from "../lib/theme";
import type { AppConfig, CustomTheme } from "../types";

interface ConfigState {
  config: AppConfig | null;
  status: "loading" | "needs-onboarding" | "ready";
  error: string | null;
  /** Set when the wizard is showing because the configured storage folder is
   *  gone, not because this is a first run. Carries the path, so the warning
   *  can name it. */
  missingBasePath: string | null;

  loadConfig: () => Promise<void>;
  completeOnboarding: (basePath: string, theme: string, language: string) => Promise<void>;
  setTheme: (theme: string) => Promise<void>;
  /** Applies the colors now and persists them, making "custom" the theme. */
  setCustomTheme: (colors: CustomTheme) => Promise<void>;
  setLanguage: (language: string) => Promise<void>;
  setBasePath: (basePath: string) => Promise<void>;
}

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: null,
  status: "loading",
  error: null,
  missingBasePath: null,

  loadConfig: async () => {
    try {
      let config = await configApi.getConfig();
      if (config && config.onboarding_completed) {
        // Theme and language are still the user's choice even when the data
        // folder is gone, so apply them before deciding where to send them.
        applyTheme(config.theme, config.custom_theme);
        applyLanguage(config.language);

        // A theme that was renamed stays renamed on disk too, otherwise the
        // migration runs again on every boot.
        const theme = normalizeTheme(config.theme);
        if (theme !== config.theme) {
          config = await configApi.updateTheme(theme);
        }

        const storage = await configApi.getStorageStatus();
        if (!storage.ready) {
          set({
            config,
            status: "needs-onboarding",
            missingBasePath: storage.base_path,
          });
          return;
        }
        set({ config, status: "ready", missingBasePath: null });
      } else {
        set({ config: null, status: "needs-onboarding", missingBasePath: null });
      }
    } catch (err) {
      set({ error: String(err), status: "needs-onboarding", missingBasePath: null });
    }
  },

  completeOnboarding: async (basePath, theme, language) => {
    const config = await configApi.completeOnboarding(basePath, theme, language);
    applyTheme(config.theme, config.custom_theme);
    applyLanguage(config.language);
    set({ config, status: "ready", missingBasePath: null });
  },

  setTheme: async (theme) => {
    applyTheme(theme, get().config?.custom_theme);
    const config = await configApi.updateTheme(theme);
    set({ config });
  },

  setCustomTheme: async (colors) => {
    applyTheme("custom", colors);
    const config = await configApi.updateCustomTheme(colors);
    set({ config });
  },

  setLanguage: async (language) => {
    applyLanguage(language);
    const config = await configApi.updateLanguage(language);
    set({ config });
  },

  setBasePath: async (basePath) => {
    const config = await configApi.updateBasePath(basePath);
    set({ config });
  },
}));
