// SPEC: book-library (LIB-09), book-reader (READ-15)

import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { ReaderPanel } from "./components/Reader/ReaderPanel";
import { SettingsPanel } from "./components/Settings/SettingsPanel";
import { LibraryPanel } from "./components/Library/LibraryPanel";
import { Wizard } from "./components/Onboarding/Wizard";
import { UpdateBanner } from "./components/Update/UpdateBanner";
import { useConfigStore } from "./store/configStore";
import { useUiStore } from "./store/uiStore";
import { useUpdateStore } from "./store/updateStore";

function App() {
  const { status, loadConfig } = useConfigStore();
  const activeView = useUiStore((s) => s.activeView);
  const initUpdates = useUpdateStore((s) => s.init);
  const checkOnBoot = useUpdateStore((s) => s.checkOnBoot);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  // Only after onboarding: the very first run should be the wizard, not a
  // network call the user never asked for.
  useEffect(() => {
    if (status !== "ready") return;
    let unlisten: (() => void) | undefined;
    initUpdates().then((stop) => {
      unlisten = stop;
      checkOnBoot();
    });
    return () => unlisten?.();
  }, [status, initUpdates, checkOnBoot]);

  if (status === "loading") {
    return <div className="h-screen w-screen bg-[var(--bg-app)]" />;
  }

  if (status === "needs-onboarding") {
    return <Wizard />;
  }

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[var(--bg-app)]">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <UpdateBanner />
        {activeView === "settings" ? (
          <SettingsPanel />
        ) : activeView === "library" ? (
          <LibraryPanel />
        ) : (
          <ReaderPanel />
        )}
      </div>
    </div>
  );
}

export default App;
