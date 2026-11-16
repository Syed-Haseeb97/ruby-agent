import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { OrbView } from "./OrbView";
import { ConsoleView } from "./ConsoleView";

function App() {
  const [windowLabel, setWindowLabel] = useState<string>(() => {
    return window.location.hash === "#console" ? "main" : "orb";
  });

  useEffect(() => {
    try {
      const win = getCurrentWindow();
      if (win && win.label) {
        setWindowLabel(win.label);
      }
    } catch {
      // Fallback
    }
  }, []);

  if (windowLabel === "orb") {
    return <OrbView />;
  }
  return <ConsoleView />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
