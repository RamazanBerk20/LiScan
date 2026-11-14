import { getCurrentWindow } from "@tauri-apps/api/window";
import { CopySimple, Minus, Square, X } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { isTauri } from "../lib/backend";
import { useI18n } from "../lib/i18n";

// GTK resizes the undecorated window from a 5 px border; these strips keep the
// bar's drag region from taking that border, and show the resize cursor.
const EDGES = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

/**
 * Minimise, maximise/restore and close for the undecorated window. The bar that
 * holds these carries `data-tauri-drag-region="deep"`, so its empty space moves
 * the window and a double-click there toggles maximise.
 */
export function WindowControls() {
  const { t } = useI18n();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const sync = () =>
      void appWindow
        .isMaximized()
        .then((value) => {
          if (disposed) return;
          setMaximized(value);
          document.documentElement.dataset.window = value ? "maximized" : "normal";
        })
        .catch(() => undefined);
    sync();
    void appWindow
      .onResized(sync)
      .then((release) => (disposed ? release() : (unlisten = release)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // A browser preview has no window to control.
  if (!isTauri()) return null;

  const run = (action: "minimize" | "toggleMaximize" | "close") =>
    void getCurrentWindow()[action]().catch(() => undefined);
  const sizeLabel = maximized ? t("restore") : t("maximize");

  return (
    <div className="window-controls">
      <button
        className="window-button"
        aria-label={t("minimize")}
        title={t("minimize")}
        onClick={() => run("minimize")}
      >
        <Minus size={15} weight="bold" />
      </button>
      <button
        className="window-button"
        aria-label={sizeLabel}
        title={sizeLabel}
        onClick={() => run("toggleMaximize")}
      >
        {maximized ? <CopySimple size={15} weight="bold" /> : <Square size={13} weight="bold" />}
      </button>
      <button
        className="window-button window-button--close"
        aria-label={t("close")}
        title={t("close")}
        onClick={() => run("close")}
      >
        <X size={15} weight="bold" />
      </button>
      {!maximized &&
        EDGES.map((edge) => (
          <span
            key={edge}
            className={`window-edge window-edge--${edge}`}
            data-tauri-drag-region="false"
            aria-hidden="true"
          />
        ))}
    </div>
  );
}
