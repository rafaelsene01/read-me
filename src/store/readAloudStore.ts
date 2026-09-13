// SPEC: read-aloud (TTS-02, TTS-03, TTS-05, TTS-06, TTS-07, TTS-16,
//       TTS-17, TTS-18, TTS-19, TTS-24, TTS-32, TTS-35, TTS-38)

import { create } from "zustand";
import { ttsApi } from "../lib/ttsApi";
import { useReaderStore } from "./readerStore";
import { useUiStore } from "./uiStore";
import type { Utterance } from "../types";

/** Where the sentence sits inside its block, so the page can mark it. */
interface Span {
  block: number;
  from: number;
  to: number;
}

interface ReadAloudState {
  status: "idle" | "loading" | "playing" | "paused";
  /** The sentence being spoken, or -1. */
  index: number;
  utterances: Utterance[];
  spans: Span[];
  /** `null` = no Piper voice for this language; the system voice reads (TTS-35). */
  voiceId: string | null;
  usingSystemVoice: boolean;
  error: string | null;
  /** Reading speed, the same stored value Settings > Voices edits (TTS-32). */
  speed: number;

  loadSpeed: () => Promise<void>;
  setSpeed: (speed: number) => Promise<void>;
  start: (from?: number) => Promise<void>;
  toggle: () => Promise<void>;
  stop: () => void;
  /** A click inside the page: block plus how far into its text (TTS-17). */
  startAt: (block: number, offset: number) => Promise<void>;
}

let audio: HTMLAudioElement | null = null;
let objectUrl: string | null = null;
/** The synthesis running one sentence ahead — the whole reason playback does
 *  not stutter, given ~236 ms per sentence against ~10x that in audio. */
let lookahead: { index: number; bytes: Promise<ArrayBuffer> } | null = null;
/** Bumped on every stop and page change; a reply from an older generation is
 *  dropped instead of playing over the new one. */
let generation = 0;
/** True while continuous reading turns the page itself. It is what tells that
 *  turn apart from one the user made, which the page subscriber below restarts
 *  on the new page. */
let autoTurn = false;

async function turnPageOnItsOwn() {
  const reader = useReaderStore.getState();
  autoTurn = true;
  try {
    await reader.goToPage(reader.page + 1);
  } finally {
    autoTurn = false;
  }
}

/** The offsets of each sentence inside its block.
 *
 *  It rebuilds the block's text by joining its sentences with one space, which
 *  is exactly what `html::visible_text` produces on the Rust side — collapsed
 *  whitespace, trimmed pieces. That shared shape is what lets an offset mean
 *  the same thing in both places without shipping a second definition. */
function spansOf(utterances: Utterance[]): Span[] {
  const out: Span[] = [];
  let block = -1;
  let at = 0;
  for (const u of utterances) {
    if (u.block !== block) {
      block = u.block;
      at = 0;
    }
    out.push({ block, from: at, to: at + u.text.length });
    at += u.text.length + 1;
  }
  return out;
}

function post(message: unknown) {
  const frame = document.querySelector<HTMLIFrameElement>("iframe[data-book-page]");
  frame?.contentWindow?.postMessage(message, "*");
}

function releaseAudio() {
  if (audio) {
    audio.onended = null;
    audio.pause();
    audio = null;
  }
  if (objectUrl) {
    URL.revokeObjectURL(objectUrl);
    objectUrl = null;
  }
}

/** The system voice, used only when no Piper voice is installed for the
 *  language. It is the difference between a first run that reads and a first
 *  run that asks for a 63 MB download before making a sound. */
function speakWithSystem(text: string, language: string, onEnd: () => void) {
  const utter = new SpeechSynthesisUtterance(text);
  const prefix = language.split(/[-_]/)[0];
  const match = window.speechSynthesis.getVoices().find((v) => v.lang.startsWith(prefix));
  if (match) utter.voice = match;
  utter.onend = onEnd;
  utter.onerror = onEnd;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utter);
}

export const useReadAloudStore = create<ReadAloudState>((set, get) => ({
  status: "idle",
  index: -1,
  utterances: [],
  spans: [],
  voiceId: null,
  usingSystemVoice: false,
  error: null,
  speed: 1,

  loadSpeed: async () => {
    const settings = await ttsApi.ttsSettings().catch(() => null);
    if (settings) set({ speed: settings.speed });
  },

  setSpeed: async (speed) => {
    set({ speed });
    try {
      await ttsApi.setTtsSpeed(speed);
    } catch (err) {
      set({ error: String(err) });
      return;
    }
    // The sentence synthesized ahead was made at the old speed. Dropping it is
    // what makes the change audible from the next sentence (TTS-32) instead of
    // the one after it.
    lookahead = null;
  },

  stop: () => {
    generation += 1;
    releaseAudio();
    lookahead = null;
    window.speechSynthesis?.cancel();
    void ttsApi.stopSpeaking().catch(() => {});
    post({ readaloud: "clear" });
    set({ status: "idle", index: -1, error: null });
  },

  start: async (from = 0) => {
    const reader = useReaderStore.getState();
    if (!reader.bookId) return;
    generation += 1;
    const mine = generation;
    releaseAudio();
    lookahead = null;
    set({ status: "loading", error: null });

    let utterances = get().utterances;
    if (utterances.length === 0) {
      utterances = await ttsApi.pageUtterances(reader.text).catch(() => []);
      if (generation !== mine) return;
      set({ utterances, spans: spansOf(utterances) });
    }
    if (utterances.length === 0) {
      // A page with no readable text is not an error: continuous reading moves
      // on rather than playing silence.
      set({ status: "idle", index: -1 });
      if (reader.page < reader.pageCount - 1) {
        await turnPageOnItsOwn();
        void get().start(0);
      }
      return;
    }

    const voiceId = await ttsApi
      .voiceFor(reader.pageLanguage, get().voiceId)
      .catch(() => null);
    if (generation !== mine) return;
    set({ voiceId, usingSystemVoice: voiceId === null });

    void playFrom(from, mine, set, get);
  },

  toggle: async () => {
    const { status } = get();
    if (status === "playing") {
      audio?.pause();
      window.speechSynthesis?.pause();
      set({ status: "paused" });
      return;
    }
    if (status === "paused") {
      // Resuming continues the sentence that was interrupted (TTS-05), because
      // the element still holds it.
      set({ status: "playing" });
      if (audio) void audio.play().catch(() => {});
      else window.speechSynthesis?.resume();
      return;
    }
    await get().start(0);
  },

  startAt: async (block, offset) => {
    const { spans, utterances, status } = get();
    if (utterances.length === 0) {
      await get().start(0);
      return;
    }
    // The sentence the click landed in; past the end of a block, the next
    // sentence forward — a click in the margin should read on, not stop.
    let index = spans.findIndex((s) => s.block === block && offset >= s.from && offset <= s.to);
    if (index < 0) index = spans.findIndex((s) => s.block >= block);
    if (index < 0) return;
    if (status === "playing" || status === "paused") get().stop();
    await get().start(index);
  },
}));

async function playFrom(
  index: number,
  mine: number,
  set: (partial: Partial<ReadAloudState>) => void,
  get: () => ReadAloudState,
) {
  const { utterances, spans, voiceId } = get();
  if (generation !== mine) return;

  if (index >= utterances.length) {
    // End of page: the next one keeps playing, and the last page stops (TTS-16).
    const reader = useReaderStore.getState();
    releaseAudio();
    if (reader.page < reader.pageCount - 1) {
      set({ utterances: [], spans: [], index: -1 });
      await turnPageOnItsOwn();
      if (generation !== mine) return;
      void get().start(0);
    } else {
      get().stop();
    }
    return;
  }

  const span = spans[index];
  set({ status: "playing", index });
  post({ readaloud: "mark", block: span.block, from: span.from, to: span.to });

  const advance = () => {
    if (generation !== mine) return;
    void playFrom(index + 1, mine, set, get);
  };

  if (!voiceId) {
    speakWithSystem(utterances[index].text, useReaderStore.getState().pageLanguage, advance);
    return;
  }

  try {
    const bytes =
      lookahead && lookahead.index === index
        ? await lookahead.bytes
        : await ttsApi.speakSentence(voiceId, utterances[index].text);
    if (generation !== mine) return;

    // One sentence ahead, started before this one plays: synthesis runs ~10x
    // faster than speech, so the next file is always ready in time.
    lookahead =
      index + 1 < utterances.length
        ? { index: index + 1, bytes: ttsApi.speakSentence(voiceId, utterances[index + 1].text) }
        : null;
    lookahead?.bytes.catch(() => {});

    releaseAudio();
    objectUrl = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
    audio = new Audio(objectUrl);
    // The element's own clock decides when the sentence ends. There is no
    // second timer to disagree with it.
    audio.onended = advance;
    await audio.play();
  } catch (err) {
    if (generation !== mine) return;
    releaseAudio();
    post({ readaloud: "clear" });
    set({ status: "idle", index: -1, error: String(err) });
  }
}

/** A page the user turns (or a language switch, which reloads the page) keeps
 *  an active reading going from the top of the new page; a paused one stops
 *  (TTS-05, changed by AD-070).
 *
 *  The previous version meant to stop here but did not: its guard,
 *  `generation === 0 || index === -1`, was false during any real playback, so
 *  the old sentence kept playing, found an empty sentence list when it ended,
 *  and continuous reading turned ONE MORE page - a manual turn skipped a page. */
useReaderStore.subscribe((state, previous) => {
  if (state.text === previous.text) return;
  useReadAloudStore.setState({ utterances: [], spans: [] });
  if (autoTurn) return; // continuous reading starts the new page itself
  const aloud = useReadAloudStore.getState();
  if (aloud.status === "idle") return;
  // Closing the book, or opening another book or another reading, is not a
  // page turn.
  if (
    state.bookId !== previous.bookId ||
    state.readingId !== previous.readingId ||
    !state.bookId
  ) {
    aloud.stop();
    return;
  }
  // `openBook` clears the text before the page arrives; wait for the page.
  if (state.text === "") return;
  if (aloud.status === "paused") aloud.stop();
  else void aloud.start(0);
});

/** Leaving the reader screen stops the reading (TTS-38): the page it follows
 *  is no longer on screen to mark, and a voice with nothing to look at is a
 *  voice the user has to hunt down to silence. */
useUiStore.subscribe((state, previous) => {
  if (previous.activeView !== "reader" || state.activeView === "reader") return;
  const aloud = useReadAloudStore.getState();
  if (aloud.status !== "idle") aloud.stop();
});
