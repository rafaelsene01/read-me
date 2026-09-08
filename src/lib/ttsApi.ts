// SPEC: read-aloud (TTS-02, TTS-06, TTS-20, TTS-21, TTS-22, TTS-24, TTS-31)

import { invoke } from "@tauri-apps/api/core";
import type { TtsSettings, Utterance, VoiceInfo } from "../types";

// `invoke` parameters go camelCase and arrive snake_case; struct fields do not
// get renamed — the same rule the reader's API already follows.
export const ttsApi = {
  listVoices: () => invoke<VoiceInfo[]>("list_voices"),
  /** Fetches piper's own manifest and caches it. Returns how many voices it
   *  holds — six built-in become 176 in 57 languages. */
  refreshVoiceCatalog: () => invoke<number>("refresh_voice_catalog"),
  ttsSettings: () => invoke<TtsSettings>("tts_settings"),
  /** `null` unpins the language's voice. */
  setTtsVoice: (language: string, voiceId: string | null) =>
    invoke<void>("set_tts_voice", { language, voiceId }),
  /** Returns the speed actually stored, after clamping. */
  setTtsSpeed: (speed: number) => invoke<number>("set_tts_speed", { speed }),
  /** Progress arrives on the `voice-download-progress` event, not here. */
  downloadVoice: (voiceId: string) => invoke<void>("download_voice", { voiceId }),
  /** Returns the bytes freed. */
  removeVoice: (voiceId: string) => invoke<number>("remove_voice", { voiceId }),
  /** The page cut into sentences, by the app's single sentence definition. */
  pageUtterances: (pageHtml: string) => invoke<Utterance[]>("page_utterances", { pageHtml }),
  /** `null` = no installed voice for this language: the caller falls back to
   *  the system voice rather than refusing to read (TTS-35). */
  voiceFor: (language: string, chosen: string | null) =>
    invoke<string | null>("voice_for", { language, chosen }),
  /** Raw WAV bytes, like `get_book_image` — no base64 on this boundary. */
  speakSentence: (voiceId: string, text: string) =>
    invoke<ArrayBuffer>("speak_sentence", { voiceId, text }),
  stopSpeaking: () => invoke<void>("stop_speaking"),
};
