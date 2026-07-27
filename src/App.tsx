import { Check, Warning, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelScan,
  chooseFolder,
  getChildren,
  getHomePath,
  getLaunchRequest,
  getScanIssues,
  getSettings,
  getView,
  listVolumes,
  saveSettings,
  startScan as startBackendScan
} from "./lib/backend";
import type {
  NodeSummary,
  ScanEvent,
  ScanIssue,
  ScanMode,
  ScanProgress,
  ScanRequest,
  ScanStatus,
  ScanSummary,
  Settings,
  ViewNode,
  VolumeInfo
} from "./types";
import { findVolumeForTarget } from "./lib/volume";
import { Overview } from "./components/Overview";
import { ScanWorkspace } from "./components/ScanWorkspace";
import {
  AboutDialog,
  AdminDialog,
  CoverageDialog,
  RemoteDialog,
  SettingsDialog
} from "./components/Dialogs";

type Screen = "overview" | "scan";

interface ToastState {
  message: string;
  tone: "normal" | "error";
}

const FALLBACK_SETTINGS: Settings = {
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
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

export function App() {
  const [screen, setScreen] = useState<Screen>("overview");
  const [settings, setSettings] = useState<Settings>(FALLBACK_SETTINGS);
  const [settingsReady, setSettingsReady] = useState(false);
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [volumesLoading, setVolumesLoading] = useState(true);
  const [homePath, setHomePath] = useState("/");

  const [request, setRequest] = useState<ScanRequest | null>(null);
  const [scanId, setScanId] = useState<string | null>(null);
  const [status, setStatus] = useState<ScanStatus>("idle");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [issues, setIssues] = useState<ScanIssue[]>([]);
  const [view, setView] = useState<ViewNode | null>(null);
  const [children, setChildren] = useState<NodeSummary[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [viewDepth, setViewDepth] = useState(4);
  const [history, setHistory] = useState<number[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);

  const [adminOpen, setAdminOpen] = useState(false);
  const [remoteOpen, setRemoteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [coverageOpen, setCoverageOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [toast, setToast] = useState<ToastState | null>(null);
  const launchHandled = useRef(false);
  const scanIdRef = useRef<string | null>(null);
  const currentNodeRef = useRef<number | null>(null);
  const viewDepthRef = useRef(4);
  const settingsRef = useRef(settings);
  const activeVolume = findVolumeForTarget(
    volumes,
    view?.displayPath ?? request?.target ?? ""
  );

  useEffect(() => {
    scanIdRef.current = scanId;
  }, [scanId]);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    currentNodeRef.current = view?.id ?? null;
  }, [view]);

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

  const loadView = useCallback(
    async (id: string, nodeId: number | null, depth = viewDepthRef.current) => {
      const [nextView, nextChildren] = await Promise.all([
        getView(id, nodeId ?? undefined, depth),
        getChildren(id, nodeId ?? undefined)
      ]);
      setView(nextView);
      setChildren(nextChildren);
      setSelectedId(null);
      currentNodeRef.current = nextView.id;
      return nextView;
    },
    []
  );

  const handleEvent = useCallback(
    async (event: ScanEvent) => {
      if (event.scanId) {
        scanIdRef.current = event.scanId;
        setScanId(event.scanId);
      }
      switch (event.event) {
        case "started":
          setStatus("scanning");
          break;
        case "progress":
          if (event.progress) setProgress(event.progress);
          break;
        case "issue":
          if (event.issue) {
            setIssues((current) =>
              current.length < 5000 ? [...current, event.issue!] : current
            );
          }
          break;
        case "completed":
        case "cancelled": {
          const id = event.scanId ?? scanIdRef.current;
          if (event.summary) setSummary(event.summary);
          setStatus(
            event.event === "cancelled"
              ? "cancelled"
              : event.summary?.status ?? "complete"
          );
          if (id && event.summary) {
            try {
              const root = await loadView(id, null, viewDepthRef.current);
              setHistory([root.id]);
              setHistoryIndex(0);
              const fullIssues = await getScanIssues(id);
              setIssues(fullIssues);
            } catch {
              if (event.event !== "cancelled") {
                notify("The scan finished, but its map could not be loaded", "error");
              }
            }
          }
          void refreshVolumes();
          break;
        }
        case "failed":
          setStatus("failed");
          notify(event.message || "The scan failed", "error");
          break;
      }
    },
    [loadView, notify, refreshVolumes]
  );

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
      setRequest(nextRequest);
      setScreen("scan");
      setStatus(mode === "administrator" ? "authorizing" : "scanning");
      setProgress(null);
      setSummary(null);
      setIssues([]);
      setView(null);
      setChildren([]);
      setSelectedId(null);
      setHistory([]);
      setHistoryIndex(-1);
      try {
        const id = await startBackendScan(nextRequest, handleEvent);
        scanIdRef.current = id;
        setScanId(id);
      } catch (error) {
        setStatus("failed");
        notify(String(error), "error");
      }
    },
    [handleEvent, notify, refreshVolumes]
  );

  useEffect(() => {
    let active = true;
    Promise.all([getSettings(), listVolumes(), getHomePath()])
      .then(async ([loadedSettings, loadedVolumes, loadedHome]) => {
        if (!active) return;
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
            setSettings(loadedSettings);
            window.setTimeout(
              () => void startScan(launch.target, launch.mode),
              0
            );
          }
        }
      })
      .catch((error) => {
        if (!active) return;
        setSettingsReady(true);
        setVolumesLoading(false);
        notify(`Some system information could not be loaded: ${error}`, "error");
      });
    return () => {
      active = false;
    };
  }, [notify, startScan]);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void chooseFolder().then((folder) => {
          if (folder) void startScan(folder);
        });
      } else if (modifier && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
      } else if (event.key === "F5" && request && status !== "scanning") {
        event.preventDefault();
        void startScan(request.target, request.mode);
      } else if (
        event.key === "Escape" &&
        (status === "scanning" || status === "authorizing") &&
        scanIdRef.current
      ) {
        void cancelScan(scanIdRef.current).catch((error) =>
          notify(String(error), "error")
        );
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [notify, request, startScan, status]);

  async function centerNode(node: Pick<NodeSummary, "id" | "kind">) {
    if (!scanId || node.kind !== "directory") return;
    try {
      await loadView(scanId, node.id);
      setHistory((current) => {
        const next = current.slice(0, historyIndex + 1);
        next.push(node.id);
        return next;
      });
      setHistoryIndex((index) => index + 1);
    } catch (error) {
      notify(String(error), "error");
    }
  }

  async function navigateHistory(index: number) {
    if (!scanId || index < 0 || index >= history.length) return;
    try {
      await loadView(scanId, history[index]);
      setHistoryIndex(index);
    } catch (error) {
      notify(String(error), "error");
    }
  }

  async function changeDepth(depth: number) {
    setViewDepth(depth);
    viewDepthRef.current = depth;
    if (!scanId || currentNodeRef.current == null) return;
    try {
      await loadView(scanId, currentNodeRef.current, depth);
    } catch (error) {
      notify(String(error), "error");
    }
  }

  function returnToOverview() {
    if (
      scanId &&
      (status === "scanning" || status === "authorizing")
    ) {
      void cancelScan(scanId).catch(() => undefined);
    }
    setScreen("overview");
  }

  const appReady = settingsReady;

  return (
    <div
      className={`app-root ${appReady ? "app-root--ready" : ""}`}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        const path =
          event.dataTransfer.getData("text/uri-list") ||
          event.dataTransfer.getData("text/plain");
        if (path) {
          const normalized = path
            .split("\n")[0]
            .trim()
            .replace(/^file:\/\//, "");
          if (normalized) void startScan(decodeURIComponent(normalized));
        }
      }}
    >
      {screen === "overview" ? (
        <Overview
          volumes={volumes}
          volumesLoading={volumesLoading}
          homePath={homePath}
          byteUnitScale={settings.byteUnitScale}
          onScan={(target) => void startScan(target)}
          onChooseFolder={() =>
            void chooseFolder().then((folder) => {
              if (folder) void startScan(folder);
            })
          }
          onRemote={() => setRemoteOpen(true)}
          onAdmin={() => setAdminOpen(true)}
          onSettings={() => setSettingsOpen(true)}
          onAbout={() => setAboutOpen(true)}
        />
      ) : (
        <ScanWorkspace
          scanId={scanId}
          target={request?.target ?? ""}
          mode={request?.mode ?? "standard"}
          status={status}
          progress={progress}
          summary={summary}
          view={view}
          children={children}
          selectedId={selectedId}
          settings={settings}
          volume={activeVolume}
          canGoBack={historyIndex > 0}
          canGoForward={historyIndex >= 0 && historyIndex < history.length - 1}
          issues={issues}
          viewDepth={viewDepth}
          onOverview={returnToOverview}
          onCancel={() => {
            if (scanId) {
              void cancelScan(scanId).catch((error) =>
                notify(String(error), "error")
              );
            }
          }}
          onRescan={() => {
            if (request) void startScan(request.target, request.mode);
          }}
          onCenter={(node) => void centerNode(node)}
          onSelect={setSelectedId}
          onBack={() => void navigateHistory(historyIndex - 1)}
          onForward={() => void navigateHistory(historyIndex + 1)}
          onUp={() => {
            if (view?.parentId != null) {
              void centerNode({ id: view.parentId, kind: "directory" });
            }
          }}
          onSettings={() => setSettingsOpen(true)}
          onCoverage={() => setCoverageOpen(true)}
          onReload={() => {
            void refreshVolumes();
            if (scanId && currentNodeRef.current != null) {
              void loadView(scanId, currentNodeRef.current);
            }
          }}
          onViewDepth={(depth) => void changeDepth(depth)}
          onNotify={notify}
        />
      )}

      <AdminDialog
        open={adminOpen}
        onOpenChange={setAdminOpen}
        defaultTarget={screen === "scan" ? view?.displayPath || request?.target || "/" : "/"}
        onStart={(target) => void startScan(target, "administrator")}
      />
      <RemoteDialog
        open={remoteOpen}
        onOpenChange={setRemoteOpen}
        onStart={(uri) => void startScan(uri)}
      />
      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        settings={settings}
        onSave={(next) => {
          setSettings(next);
          void saveSettings(next).catch((error) =>
            notify(`Settings could not be saved: ${error}`, "error")
          );
        }}
      />
      <CoverageDialog
        open={coverageOpen}
        onOpenChange={setCoverageOpen}
        issues={issues}
      />
      <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />

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
            aria-label="Dismiss"
          >
            <X size={16} />
          </button>
        </div>
      )}
    </div>
  );
}
