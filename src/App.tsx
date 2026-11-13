import { Check, Warning, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import {
  CHILDREN_PAGE_SIZE,
  cancelScan,
  chooseFolder,
  clearHistory,
  compareScan,
  getChildren,
  getHomePath,
  getLaunchRequest,
  getReclaimable,
  getScanIssues,
  getSettings,
  getSystemLanguages,
  getView,
  listSnapshots,
  listVolumes,
  performFileAction,
  releaseScan,
  saveSettings,
  startScan as startBackendScan
} from "./lib/backend";
import type {
  Lens,
  NodeDetail,
  NodeSummary,
  ScanMode,
  ScanRequest,
  Settings,
  SnapshotInfo,
  VolumeInfo
} from "./types";
import { findVolumeForTarget } from "./lib/volume";
import { createLatest } from "./lib/latest";
import { dropTarget } from "./lib/dropTarget";
import { I18nProvider, resolveLocale, translate } from "./lib/i18n";
import { canStop, initialScan, scanReducer } from "./lib/scanMachine";
import { Overview } from "./components/Overview";
import { ScanWorkspace } from "./components/ScanWorkspace";
import {
  AboutDialog,
  AdminDialog,
  CoverageDialog,
  DeleteDialog,
  RemoteDialog,
  SettingsDialog
} from "./components/Dialogs";

/** At most one dialog is open; the delete payload is frozen when it opens. */
type DialogState =
  | null
  | { kind: "admin"; target: string }
  | { kind: "remote"; uri: string }
  | { kind: "settings" }
  | { kind: "coverage" }
  | { kind: "about" }
  | { kind: "delete"; scanId: string; node: NodeDetail };

interface ToastState {
  message: string;
  tone: "normal" | "error";
}

const FALLBACK_SETTINGS: Settings = {
  language: "system",
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
  keepHistory: true,
  scanOptions: {
    crossFilesystems: false,
    includeRemoteMounts: false,
    includeRemovable: true,
    showSmallFiles: false,
    exclusions: ["/proc", "/sys", "/dev", "/run"]
  }
};

function resolveTheme(preference: Settings["theme"]) {
  if (preference !== "system") return preference;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

async function loadView(scanId: string, nodeId: number | null, depth: number) {
  const [view, children] = await Promise.all([
    getView(scanId, nodeId ?? undefined, depth),
    getChildren(scanId, nodeId ?? undefined)
  ]);
  return { view, children };
}

export function App() {
  const [settings, setSettings] = useState<Settings>(FALLBACK_SETTINGS);
  const [settingsReady, setSettingsReady] = useState(false);
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [volumesLoading, setVolumesLoading] = useState(true);
  const [homePath, setHomePath] = useState("/");
  const [systemLanguages, setSystemLanguages] = useState<string[]>();

  const [scan, dispatch] = useReducer(scanReducer, initialScan);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [viewDepth, setViewDepth] = useState(4);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [snapshots, setSnapshots] = useState<SnapshotInfo[]>([]);
  const [lens, setLens] = useState<Lens | null>(null);
  const [historyVersion, setHistoryVersion] = useState(0);

  const systemLocale = resolveLocale("system", systemLanguages);
  const locale = resolveLocale(settings.language, systemLanguages);
  const localeRef = useRef(locale);
  const launchHandled = useRef(false);
  const genRef = useRef(0);
  const scanRef = useRef(scan);
  const viewDepthRef = useRef(4);
  // Folder loads are numbered so a slow older reply cannot replace a newer one.
  const showLatest = useRef(createLatest());
  const settingsRef = useRef(settings);
  scanRef.current = scan;

  const ready = scan.kind === "ready" ? scan : null;
  const request = scan.kind === "idle" ? null : scan.request;
  const view = ready?.view ?? null;
  const activeVolume = findVolumeForTarget(
    volumes,
    view?.displayPath ?? request?.target ?? ""
  );

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  // Each finished scan starts without a lens and learns which earlier scans exist.
  // Administrator scans are never recorded, so they have nothing to compare with.
  const readyScanId = ready && !ready.partial ? ready.scanId : null;
  const comparable = !ready?.summary?.elevated;
  useEffect(() => {
    setLens(null);
    setSnapshots([]);
    if (!readyScanId || !comparable) return;
    let alive = true;
    listSnapshots(readyScanId)
      .then((found) => alive && setSnapshots(found))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [readyScanId, comparable, historyVersion]);

  useEffect(() => {
    localeRef.current = locale;
  }, [locale]);

  useEffect(() => {
    viewDepthRef.current = viewDepth;
  }, [viewDepth]);

  const notify = useCallback(
    (message: string, tone: "normal" | "error" = "normal") => {
      setToast({ message, tone });
    },
    []
  );

  const refreshVolumes = useCallback(async () => {
    try {
      setVolumes(await listVolumes());
      setVolumesLoading(false);
    } catch {
      // Capacity information is helpful but must never block a scan.
    }
  }, []);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 4200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = resolveTheme(settings.theme);
      document.documentElement.dataset.theme = resolved;
      document.documentElement.style.colorScheme = resolved;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings.theme]);

  const startScan = useCallback(
    async (target: string, mode: ScanMode = "standard") => {
      void refreshVolumes();
      const currentSettings = settingsRef.current;
      const nextRequest: ScanRequest = {
        target,
        mode,
        options: {
          ...currentSettings.scanOptions,
          includeRemoteMounts:
            mode === "administrator"
              ? false
              : currentSettings.scanOptions.includeRemoteMounts
        }
      };
      // The backend stops the window's previous scan; the new gen drops its late events.
      const gen = ++genRef.current;
      setSelectedId(null);
      dispatch({ type: "start", gen, request: nextRequest });
      try {
        const scanId = await startBackendScan(
          nextRequest,
          (event) => dispatch({ type: "event", gen, event }),
          gen
        );
        dispatch({ type: "assigned", gen, scanId });
      } catch (error) {
        dispatch({ type: "startFailed", gen, message: String(error) });
      }
    },
    [refreshVolumes]
  );

  // Cancel as soon as the scan id is known, even if Stop was pressed before.
  const cancellingId = scan.kind === "cancelling" ? scan.scanId : null;
  useEffect(() => {
    if (cancellingId) void cancelScan(cancellingId).catch(() => undefined);
  }, [cancellingId]);

  // Load the finished tree's map for the current scan only.
  const loading = scan.kind === "loading" ? scan : null;
  useEffect(() => {
    if (!loading) return;
    const { gen, scanId, partial } = loading;
    void (async () => {
      try {
        const [{ view, children }, issues] = await Promise.all([
          loadView(scanId, null, viewDepthRef.current),
          getScanIssues(scanId)
        ]);
        setSelectedId(null);
        dispatch({ type: "loaded", gen, view, children, issues });
      } catch {
        dispatch({ type: "loadFailed", gen });
        if (!partial && gen === genRef.current) {
          notify(translate(localeRef.current, "mapLoadFailed"), "error");
        }
      }
      void refreshVolumes();
    })();
    // Runs once per loading scan; `loading` itself changes identity on every render.
  }, [loading?.gen, loading?.scanId]);

  useEffect(() => {
    let active = true;
    Promise.all([
      getSettings(),
      listVolumes(),
      getHomePath(),
      getSystemLanguages()
    ])
      .then(
        async ([
          loadedSettings,
          loadedVolumes,
          loadedHome,
          loadedSystemLanguages
        ]) => {
          if (!active) return;
          setSystemLanguages(loadedSystemLanguages);
          localeRef.current = resolveLocale(
            loadedSettings.language,
            loadedSystemLanguages
          );
          settingsRef.current = loadedSettings;
          setSettings(loadedSettings);
          setSettingsReady(true);
          setVolumes(loadedVolumes);
          setVolumesLoading(false);
          setHomePath(loadedHome);
          if (!launchHandled.current) {
            launchHandled.current = true;
            const launch = await getLaunchRequest();
            if (active && launch) {
              window.setTimeout(
                () => void startScan(launch.target, launch.mode),
                0
              );
            }
          }
        }
      )
      .catch((error) => {
        if (!active) return;
        setSettingsReady(true);
        setVolumesLoading(false);
        notify(
          translate(localeRef.current, "systemInformationFailed", {
            error: String(error)
          }),
          "error"
        );
      });
    return () => {
      active = false;
    };
  }, [notify, startScan]);

  const chooseAndScan = useCallback(() => {
    void chooseFolder().then((folder) => {
      if (folder) void startScan(folder);
    });
  }, [startScan]);

  const cancel = useCallback(() => {
    dispatch({ type: "cancel", gen: scanRef.current.gen });
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      // Dialogs own the keyboard; Escape there closes the dialog only.
      if (dialog) return;
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === "o") {
        event.preventDefault();
        chooseAndScan();
      } else if (modifier && event.key === ",") {
        event.preventDefault();
        setDialog({ kind: "settings" });
      } else if (
        event.key === "F5" &&
        (scan.kind === "ready" || scan.kind === "failed")
      ) {
        event.preventDefault();
        void startScan(scan.request.target, scan.request.mode);
      } else if (event.key === "Escape" && canStop(scan)) {
        cancel();
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [cancel, chooseAndScan, dialog, scan, startScan]);

  /** Loads a folder of the ready scan; results from an older scan are dropped. */
  async function showNode(
    nodeId: number,
    nav: "push" | "replace" | number,
    depth = viewDepthRef.current
  ) {
    const current = scanRef.current;
    if (current.kind !== "ready" || !current.scanId) return;
    const ticket = showLatest.current.next();
    try {
      const { view, children } = await loadView(current.scanId, nodeId, depth);
      if (!showLatest.current.isCurrent(ticket)) return;
      if (nav !== "replace") setSelectedId(null);
      dispatch({ type: "view", gen: current.gen, view, children, nav });
    } catch (error) {
      notify(String(error), "error");
    }
  }

  function centerNode(node: Pick<NodeSummary, "id" | "kind">) {
    if (node.kind === "directory") void showNode(node.id, "push");
  }

  function navigateHistory(index: number) {
    if (!ready || index < 0 || index >= ready.nav.history.length) return;
    void showNode(ready.nav.history[index], index);
  }

  function changeDepth(depth: number) {
    setViewDepth(depth);
    viewDepthRef.current = depth;
    if (view) void showNode(view.id, "replace", depth);
  }

  async function loadMoreChildren() {
    if (!ready?.scanId || !ready.view) return;
    const { gen, scanId } = ready;
    const nodeId = ready.view.id;
    const offset = ready.children.items.length;
    try {
      const page = await getChildren(scanId, nodeId, offset, CHILDREN_PAGE_SIZE);
      dispatch({ type: "more", gen, nodeId, offset, page });
    } catch (error) {
      notify(String(error), "error");
    }
  }

  /** Applies a lens only if the scan it was computed for is still the one shown. */
  async function openLens(scanId: string, load: () => Promise<Lens>, failure: "compareFailed" | "reclaimFailed") {
    try {
      const next = await load();
      const current = scanRef.current;
      if (current.kind === "ready" && current.scanId === scanId) setLens(next);
    } catch (error) {
      notify(translate(localeRef.current, failure, { error: String(error) }), "error");
    }
  }

  function returnToOverview() {
    const scanId = "scanId" in scan ? scan.scanId : null;
    if (scanId) void releaseScan(scanId).catch(() => undefined);
    setSelectedId(null);
    dispatch({ type: "reset", gen: ++genRef.current });
  }

  const closeDialog = (open: boolean) => {
    if (!open) setDialog(null);
  };
  const deleting = dialog?.kind === "delete" ? dialog : null;

  return (
    <I18nProvider locale={locale}>
      <div
        className={`app-root ${settingsReady ? "app-root--ready" : ""}`}
        data-scan-state={scan.kind}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          if (dialog) return;
          const target = dropTarget(event.dataTransfer.getData("text/uri-list"));
          if (target && "scan" in target) void startScan(target.scan);
          else if (target) setDialog({ kind: "remote", uri: target.remote });
        }}
      >
        {scan.kind === "idle" ? (
          <Overview
            volumes={volumes}
            volumesLoading={volumesLoading}
            homePath={homePath}
            byteUnitScale={settings.byteUnitScale}
            onScan={(target) => void startScan(target)}
            onChooseFolder={chooseAndScan}
            onRemote={() => setDialog({ kind: "remote", uri: "sftp://" })}
            onAdmin={() => setDialog({ kind: "admin", target: "/" })}
            onSettings={() => setDialog({ kind: "settings" })}
            onAbout={() => setDialog({ kind: "about" })}
          />
        ) : (
          <ScanWorkspace
            analysis={
              readyScanId
                ? {
                    snapshots,
                    lens,
                    onCompare: (timestamp) =>
                      void openLens(
                        readyScanId,
                        async () => ({
                          kind: "compare",
                          diff: await compareScan(readyScanId, timestamp)
                        }),
                        "compareFailed"
                      ),
                    onToggleReclaim: () =>
                      lens?.kind === "reclaim"
                        ? setLens(null)
                        : void openLens(
                            readyScanId,
                            async () => ({
                              kind: "reclaim",
                              data: await getReclaimable(readyScanId)
                            }),
                            "reclaimFailed"
                          ),
                    onExitLens: () => setLens(null)
                  }
                : undefined
            }
            scan={scan}
            selectedId={selectedId}
            settings={settings}
            volume={activeVolume}
            viewDepth={viewDepth}
            onOverview={returnToOverview}
            onCancel={cancel}
            onRescan={() => void startScan(scan.request.target, scan.request.mode)}
            onCenter={centerNode}
            onSelect={setSelectedId}
            onBack={() => ready && navigateHistory(ready.nav.index - 1)}
            onForward={() => ready && navigateHistory(ready.nav.index + 1)}
            onUp={() => {
              if (view?.parentId != null) {
                centerNode({ id: view.parentId, kind: "directory" });
              }
            }}
            onSettings={() => setDialog({ kind: "settings" })}
            onCoverage={() => setDialog({ kind: "coverage" })}
            onReload={() => {
              void refreshVolumes();
              if (view) void showNode(view.id, "replace");
            }}
            onMoreChildren={() => void loadMoreChildren()}
            onRequestDelete={(node) => {
              if (ready?.scanId) {
                setDialog({ kind: "delete", scanId: ready.scanId, node });
              }
            }}
            onViewDepth={changeDepth}
            onNotify={notify}
          />
        )}

        <AdminDialog
          open={dialog?.kind === "admin"}
          onOpenChange={closeDialog}
          defaultTarget={dialog?.kind === "admin" ? dialog.target : "/"}
          onStart={(target) => void startScan(target, "administrator")}
        />
        <RemoteDialog
          open={dialog?.kind === "remote"}
          onOpenChange={closeDialog}
          initialUri={dialog?.kind === "remote" ? dialog.uri : "sftp://"}
          onStart={(uri) => void startScan(uri)}
        />
        <SettingsDialog
          open={dialog?.kind === "settings"}
          onOpenChange={closeDialog}
          settings={settings}
          systemLocale={systemLocale}
          onClearHistory={() =>
            void clearHistory()
              .then(() => {
                notify(translate(localeRef.current, "historyCleared"));
                setHistoryVersion((version) => version + 1);
              })
              .catch((error) =>
                notify(
                  translate(localeRef.current, "historyClearFailed", {
                    error: String(error)
                  }),
                  "error"
                )
              )
          }
          onSave={(next) => {
            setSettings(next);
            void saveSettings(next).catch((error) =>
              notify(
                translate(localeRef.current, "settingsSaveFailed", {
                  error: String(error)
                }),
                "error"
              )
            );
          }}
        />
        <CoverageDialog
          open={dialog?.kind === "coverage"}
          onOpenChange={closeDialog}
          issues={
            scan.kind === "ready"
              ? scan.issues
              : scan.kind === "scanning" || scan.kind === "cancelling"
                ? scan.liveIssues
                : []
          }
          summary={ready?.summary ?? null}
        />
        <AboutDialog open={dialog?.kind === "about"} onOpenChange={closeDialog} />
        <DeleteDialog
          node={deleting?.node ?? null}
          open={deleting !== null}
          onOpenChange={closeDialog}
          onConfirm={async () => {
            if (!deleting) return;
            const { scanId, node } = deleting;
            try {
              await performFileAction(scanId, node.id, "delete");
              notify(translate(localeRef.current, "permanentlyDeleted", { name: node.name }));
              const current = scanRef.current;
              if (current.kind === "ready" && current.scanId === scanId && current.view) {
                setSelectedId(null);
                void showNode(current.view.id, "replace");
              }
            } catch (error) {
              notify(String(error), "error");
            }
          }}
        />

        {toast && (
          <div className={`toast toast--${toast.tone}`} role="status">
            <span>
              {toast.tone === "error" ? (
                <Warning size={17} weight="fill" />
              ) : (
                <Check size={17} weight="bold" />
              )}
            </span>
            <p>{toast.message}</p>
            <button
              className="icon-button"
              onClick={() => setToast(null)}
              aria-label={translate(locale, "dismiss")}
            >
              <X size={16} />
            </button>
          </div>
        )}
      </div>
    </I18nProvider>
  );
}
