import { invoke } from "@tauri-apps/api/core";
// SPEC: settings-storage-i18n (CFG-01, CFG-05, CFG-06, CFG-07, CFG-09)

import type { AppConfig, CustomTheme, StorageStatus } from "../types";

export const configApi = {
  getConfig: () => invoke<AppConfig | null>("get_app_config"),
  getStorageStatus: () => invoke<StorageStatus>("get_storage_status"),
  getDefaultBasePath: () => invoke<string>("get_default_base_path"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  completeOnboarding: (base_path: string, theme: string, language: string) =>
    invoke<AppConfig>("complete_onboarding", { basePath: base_path, theme, language }),
  updateTheme: (theme: string) => invoke<AppConfig>("update_theme", { theme }),
  /** Also makes "custom" the active theme. Refused unless every color is #rrggbb. */
  updateCustomTheme: ({ background, text, accent }: CustomTheme) =>
    invoke<AppConfig>("update_custom_theme", { background, text, accent }),
  updateLanguage: (language: string) => invoke<AppConfig>("update_language", { language }),
  updateBasePath: (newBasePath: string) =>
    invoke<AppConfig>("update_base_path", { newBasePath }),
};
