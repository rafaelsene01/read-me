import { create } from "zustand";

// SPEC: self-contained-runtime (SELF-01), book-library (LIB-09),
//       book-reader (READ-15), reading-history (HIST-01)

// `"chat"` is gone, not merely unrouted: a union member nothing renders is a
// state the app can enter and show nothing (AD-052 item 4). `"runtime"` went
// the same way when it became tabs inside Settings (SHELL-09).
export type ActiveView = "reader" | "settings" | "library";

interface UiState {
  activeView: ActiveView;
  setActiveView: (view: ActiveView) => void;
}

export const useUiStore = create<UiState>((set) => ({
  // The app opens on the reader, which is what it is for now.
  activeView: "reader",
  setActiveView: (view) => set({ activeView: view }),
}));
