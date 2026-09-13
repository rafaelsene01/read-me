// SPEC: app-shell (SHELL-01, SHELL-09), book-library (LIB-09), reading-history (HIST-01)

import { ReadingList } from "./ReadingList";
import { LibrarySection } from "./LibrarySection";
import { SettingsSection } from "./SettingsSection";

export function Sidebar() {
  return (
    <aside className="flex h-full w-72 shrink-0 flex-col bg-[var(--bg-sidebar)] text-[var(--text-primary)]">
      <ReadingList />
      <LibrarySection />
      <SettingsSection />
    </aside>
  );
}
