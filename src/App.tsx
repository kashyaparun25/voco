import { useEffect, useState } from "react";
import { Theme } from "@astryxdesign/core";
import { neutralTheme } from "@astryxdesign/theme-neutral";
import MainWindow from "./windows/MainWindow";
import PillWindow from "./windows/PillWindow";
import Toast from "./components/common/Toast";
import { useTheme } from "./hooks/useTheme";
import { getCurrentWindow } from "@tauri-apps/api/window";

function App() {
  const { themeId, setThemeId, mode } = useTheme();
  const [windowLabel, setWindowLabel] = useState<string>("main");

  useEffect(() => {
    try {
      const label = getCurrentWindow().label;
      setWindowLabel(label);
      document.documentElement.setAttribute("data-window", label);
    } catch (e) {
      console.warn("Tauri getCurrentWindow not available, defaulting to main window.", e);
    }
  }, []);

  return (
    <Theme theme={neutralTheme} mode={mode}>
      {/* Re-assert the app's theme tokens on a wrapper INSIDE the astryx Theme.
          Astryx redefines --color-accent / --color-text-* on its own wrapper
          (neutral palette); without this, app content inherits astryx's neutral
          values instead of the selected theme's. `display: contents` keeps
          layout unchanged while custom-property inheritance still flows through. */}
      <div id="voco-theme-root" data-theme={themeId} style={{ display: "contents" }}>
        {windowLabel === "pill" ? (
          <PillWindow />
        ) : (
          <>
            <MainWindow activeThemeId={themeId} onSelectTheme={setThemeId} />
            <Toast />
          </>
        )}
      </div>
    </Theme>
  );
}

export default App;
