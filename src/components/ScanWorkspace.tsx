import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Tooltip from "@radix-ui/react-tooltip";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  ArrowCounterClockwise,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Broom,
  CaretRight,
  ChartDonut,
  Check,
  Clipboard,
  DotsThree,
  Folder,
  FolderOpen,
  Gear,
  House,
  GitDiff,
  Info,
  List,
  LockKey,
  MagnifyingGlassMinus,
  MagnifyingGlassPlus,
  Stop,
  Terminal,
  Trash,
  Warning,
  X
} from "@phosphor-icons/react";
import { useEffect, useMemo, useState } from "react";
import type {
  Lens,
  NodeDetail,
  NodeSummary,
  Settings,
  SnapshotInfo,
  VolumeInfo
} from "../types";
import { getNode, performFileAction } from "../lib/backend";
import { canStop, isBusy, type Scan } from "../lib/scanMachine";
import { formatBytes, formatCount } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { AdminBadge, LiScanMark } from "./Brand";
import { deltaIndex } from "../lib/lens";
import { GROW_COLOR, RadialMap, SHRINK_COLOR, type MapLens } from "./RadialMap";
import { ComparePanel, ReclaimPanel, formatDate } from "./LensPanel";
import { CapacityStrip } from "./CapacityStrip";
import { WindowControls } from "./WindowControls";

/** Compare and Reclaimable views of a finished scan. */
export interface Analysis {
  snapshots: SnapshotInfo[];
  lens: Lens | null;
  onCompare: (timestamp: number) => void;
  onToggleReclaim: () => void;
  onExitLens: () => void;
}

interface ScanWorkspaceProps {
  analysis?: Analysis;
  scan: Exclude<Scan, { kind: "idle" }>;
  selectedId: number | null;
  settings: Settings;
  volume: VolumeInfo | null;
  viewDepth: number;
  onOverview: () => void;
  onCancel: () => void;
  onRescan: () => void;
  onCenter: (node: Pick<NodeSummary, "id" | "kind">) => void;
  onSelect: (nodeId: number | null) => void;
  onBack: () => void;
  onForward: () => void;
  onUp: () => void;
  onSettings: () => void;
  onCoverage: () => void;
  onReload: () => void;
  onMoreChildren: () => void;
  onRequestDelete: (node: NodeDetail) => void;
  onViewDepth: (depth: number) => void;
  onNotify: (message: string, tone?: "normal" | "error") => void;
}

function ToolbarButton({
  label,
  disabled,
  onClick,
  children
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          className="icon-button toolbar-button"
          disabled={disabled}
          onClick={onClick}
          aria-label={label}
        >
          {children}
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content className="tooltip-content" sideOffset={7}>
          {label}
          <Tooltip.Arrow className="tooltip-arrow" />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

function ItemMenu({
  node,
  onAction,
  onCenter
}: {
  node: NodeSummary;
  onAction: (action: string) => void;
  onCenter: () => void;
}) {
  const { locale, t } = useI18n();
  return (
    <DropdownMenu.Root dir={locale === "ar" ? "rtl" : "ltr"}>
      <DropdownMenu.Trigger asChild>
        <button
          className="icon-button row-menu-button"
          aria-label={t("itemActions")}
        >
          <DotsThree size={19} weight="bold" />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          className="menu-content"
          sideOffset={4}
          align="end"
        >
          <DropdownMenu.Item className="menu-item" onSelect={() => onAction("open")}>
            <FolderOpen size={17} />
            {t("open")}
          </DropdownMenu.Item>
          {node.kind === "directory" && (
            <DropdownMenu.Item className="menu-item" onSelect={onCenter}>
              <ChartDonut size={17} />
              {t("centerMapHere")}
            </DropdownMenu.Item>
          )}
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("reveal")}
          >
            <Folder size={17} />
            {t("openInFileManager")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("terminal")}
          >
            <Terminal size={17} />
            {t("openTerminalHere")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("copy_path")}
          >
            <Clipboard size={17} />
            {t("copyPath")}
          </DropdownMenu.Item>
          <DropdownMenu.Separator className="menu-separator" />
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("trash")}
          >
            <Trash size={17} />
            {t("moveToTrash")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item menu-item--danger"
            onSelect={() => onAction("request_delete")}
          >
            <Warning size={17} />
            {t("permanentlyDelete")}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

export function ScanWorkspace({
  analysis,
  scan,
  selectedId,
  settings,
  volume,
  viewDepth,
  onOverview,
  onCancel,
  onRescan,
  onCenter,
  onSelect,
  onBack,
  onForward,
  onUp,
  onSettings,
  onCoverage,
  onReload,
  onMoreChildren,
  onRequestDelete,
  onViewDepth,
  onNotify
}: ScanWorkspaceProps) {
  const { locale, t } = useI18n();
  const [selected, setSelected] = useState<NodeDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const ready = scan.kind === "ready" ? scan : null;
  // View, children and the scan id always come from the same ready state.
  const scanId = ready?.scanId ?? null;
  const view = ready?.view ?? null;
  const children = ready?.children ?? { items: [], total: 0 };
  const summary =
    scan.kind === "ready" || scan.kind === "loading" ? scan.summary : null;
  const progress =
    scan.kind === "scanning" || scan.kind === "cancelling" ? scan.progress : null;
  const target = scan.request.target;
  const mode = scan.request.mode;
  const issueCount = summary
    ? summary.issues + summary.excluded
    : scan.kind === "scanning" || scan.kind === "cancelling"
      ? scan.liveIssues.length
      : 0;
  const lens = analysis?.lens ?? null;
  const mapLens = useMemo<MapLens | null>(() => {
    if (lens?.kind === "compare") return { kind: "compare", ...deltaIndex(lens.diff.rows) };
    if (lens?.kind === "reclaim") {
      return { kind: "reclaim", paths: lens.data.matches.map((match) => match.path) };
    }
    return null;
  }, [lens]);
  const canGoBack = (ready?.nav.index ?? 0) > 0;
  const canGoForward =
    ready != null &&
    ready.nav.index >= 0 &&
    ready.nav.index < ready.nav.history.length - 1;

  useEffect(() => {
    if (!scanId || selectedId == null || selectedId > Number.MAX_SAFE_INTEGER) {
      setSelected(null);
      return;
    }
    let alive = true;
    setDetailLoading(true);
    getNode(scanId, selectedId)
      .then((node) => {
        if (alive) setSelected(node);
      })
      .catch(() => {
        if (alive) setSelected(null);
      })
      .finally(() => {
        if (alive) setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [scanId, selectedId]);

  const running = isBusy(scan);
  const stoppable = canStop(scan);
  const partial = ready?.partial ?? (scan.kind === "loading" && scan.partial);
  const status =
    scan.kind === "ready" || scan.kind === "loading"
      ? partial
        ? "cancelled"
        : (summary?.status ?? "complete")
      : scan.kind;

  async function runAction(action: string, node = selected) {
    if (!scanId || !node) return;
    if (action === "request_delete") {
      onRequestDelete(node);
      return;
    }
    try {
      const output = await performFileAction(scanId, node.id, action);
      if (action === "copy_path" && output) {
        await writeText(output);
        onNotify(t("pathCopied"));
      } else if (action === "trash") {
        onNotify(t("movedToTrash", { name: node.name }));
        onSelect(null);
        onReload();
      }
    } catch (error) {
      onNotify(String(error), "error");
    }
  }

  const statusLabel = {
    authorizing: t("statusWaitingAuthentication"),
    scanning: t("statusScanning"),
    cancelling: t("statusCancelling"),
    loading: t("preparingMap"),
    complete_with_issues: t("statusCompletedWithIssues"),
    cancelled: t("statusPartialScan"),
    failed: t("statusScanFailed"),
    complete: t("statusScanComplete"),
    idle: t("statusScanComplete")
  }[status];

  const progressScreen = (heading: string, prompt?: string) => (
    <main className="scanning-state">
      <div className="scanning-visual" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <h1>{heading}</h1>
      {prompt && <p>{prompt}</p>}
      <p className="scanning-path" dir="auto">
        {progress?.currentPath || target}
      </p>
      <div className="scanning-stats">
        <div>
          <strong>
            {formatCount(progress?.directoriesScanned ?? 0, locale)}
          </strong>
          <span>{t("folders")}</span>
        </div>
        <div>
          <strong>{formatCount(progress?.filesScanned ?? 0, locale)}</strong>
          <span>{t("files")}</span>
        </div>
        <div>
          <strong>
            {formatBytes(
              progress?.bytesScanned ?? 0,
              settings.byteUnitScale,
              locale
            )}
          </strong>
          <span>{t("counted")}</span>
        </div>
      </div>
      {stoppable && (
        <button className="button button--quiet-border" onClick={onCancel}>
          <X size={16} />
          {t("cancelScan")}
        </button>
      )}
    </main>
  );

  return (
    <div
      className={`workspace-shell ${
        volume ? "workspace-shell--capacity" : ""
      }`}
    >
      <header className="workspace-header" data-tauri-drag-region="deep">
        <button
          className="brand-button"
          onClick={onOverview}
          aria-label={t("overview")}
        >
          <LiScanMark compact />
        </button>
        <div className="workspace-nav">
          <ToolbarButton label={t("back")} disabled={!canGoBack} onClick={onBack}>
            <ArrowLeft size={18} />
          </ToolbarButton>
          <ToolbarButton
            label={t("forward")}
            disabled={!canGoForward}
            onClick={onForward}
          >
            <ArrowRight size={18} />
          </ToolbarButton>
          <ToolbarButton
            label={t("parentFolder")}
            disabled={!view?.parentId || running}
            onClick={onUp}
          >
            <ArrowUp size={18} />
          </ToolbarButton>
        </div>
        {/* The path stays selectable text; only the gaps around it move the window. */}
        <div
          className="path-bar"
          title={view?.displayPath ?? target}
          dir="auto"
          data-tauri-drag-region="false"
        >
          <Folder size={16} weight="fill" />
          <span>{view?.displayPath ?? target}</span>
        </div>
        <div className="workspace-tools">
          {analysis && ready && view && scanId && (
            <>
              {analysis.snapshots.length > 0 && (
                <DropdownMenu.Root dir={locale === "ar" ? "rtl" : "ltr"}>
                  <DropdownMenu.Trigger asChild>
                    <button
                      className={`button button--quiet-border lens-button ${
                        lens?.kind === "compare" ? "lens-button--active" : ""
                      }`}
                      aria-label={t("compareWithEarlier")}
                    >
                      <GitDiff size={16} />
                      <span>{t("compare")}</span>
                    </button>
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Portal>
                    <DropdownMenu.Content
                      className="menu-content"
                      sideOffset={4}
                      align="end"
                    >
                      <DropdownMenu.Label className="menu-label">
                        {t("compareWithEarlier")}
                      </DropdownMenu.Label>
                      {analysis.snapshots.map((snapshot) => (
                        <DropdownMenu.Item
                          key={snapshot.timestamp}
                          className="menu-item menu-item--split"
                          onSelect={() => analysis.onCompare(snapshot.timestamp)}
                        >
                          <span>{formatDate(snapshot.timestamp, locale)}</span>
                          <small>
                            {formatBytes(snapshot.total, settings.byteUnitScale, locale)}
                          </small>
                        </DropdownMenu.Item>
                      ))}
                    </DropdownMenu.Content>
                  </DropdownMenu.Portal>
                </DropdownMenu.Root>
              )}
              <button
                className={`button button--quiet-border lens-button ${
                  lens?.kind === "reclaim" ? "lens-button--active" : ""
                }`}
                aria-pressed={lens?.kind === "reclaim"}
                aria-label={t("reclaimable")}
                onClick={analysis.onToggleReclaim}
              >
                <Broom size={16} />
                <span>{t("reclaimable")}</span>
              </button>
            </>
          )}
          {running ? (
            <button
              className="button button--stop"
              onClick={onCancel}
              disabled={!stoppable}
            >
              <Stop size={16} weight="fill" />
              {t("stop")}
            </button>
          ) : (
            <ToolbarButton label={t("rescan")} onClick={onRescan}>
              <ArrowCounterClockwise size={18} />
            </ToolbarButton>
          )}
          <ToolbarButton
            label={t("zoomIn")}
            onClick={() => onViewDepth(Math.max(2, viewDepth - 1))}
          >
            <MagnifyingGlassPlus size={18} />
          </ToolbarButton>
          <ToolbarButton
            label={t("zoomOut")}
            onClick={() => onViewDepth(Math.min(6, viewDepth + 1))}
          >
            <MagnifyingGlassMinus size={18} />
          </ToolbarButton>
          <ToolbarButton label={t("settings")} onClick={onSettings}>
            <Gear size={19} />
          </ToolbarButton>
        </div>
        <WindowControls />
        {/* While a scan runs, a hair-thin line travels along the bar's bottom edge. */}
        {running && (
          <div className="scan-progress-track" aria-hidden="true">
            <span />
          </div>
        )}
      </header>

      <div className="workspace-summary">
        <div className={`scan-status scan-status--${status}`}>
          <div className="scan-status__primary">
            <span
              className={`status-symbol ${
                running ? "status-symbol--running" : ""
              }`}
            >
              {running ? (
                <span className="status-symbol__pulse" />
              ) : status === "complete" ? (
                <Check size={15} weight="bold" />
              ) : status === "complete_with_issues" || status === "failed" ? (
                <Warning size={15} weight="fill" />
              ) : (
                <Info size={15} />
              )}
            </span>
            <strong>{statusLabel}</strong>
            {mode === "administrator" && <AdminBadge />}
          </div>
          <div className="scan-status__metrics">
            {progress && running && (
              <>
                <span>
                  {t("filesCount", {
                    count: formatCount(progress.filesScanned, locale)
                  })}
                </span>
                <span>
                  {formatBytes(
                    progress.bytesScanned,
                    settings.byteUnitScale,
                    locale
                  )}
                </span>
              </>
            )}
            {summary && (
              <>
                <span>
                  {t("filesCount", {
                    count: formatCount(summary.files, locale)
                  })}
                </span>
                <span>
                  {formatBytes(
                    summary.allocatedBytes,
                    settings.byteUnitScale,
                    locale
                  )}
                </span>
              </>
            )}
            <button
              className="coverage-button"
              onClick={onCoverage}
              disabled={status === "failed"}
            >
              {running ? (
                <>
                  <Info size={14} /> {t("coveragePending")}
                </>
              ) : status === "failed" ? (
                <>
                  <Warning size={14} /> {t("coverageUnavailable")}
                </>
              ) : status === "cancelled" ? (
                <>
                  <Info size={14} /> {t("partialCoverage")}
                </>
              ) : issueCount === 0 ? (
                <>
                  <Check size={14} /> {t("fullCoverage")}
                </>
              ) : (
                <>
                  <Info size={14} />{" "}
                  {t("reportsCount", {
                    count: formatCount(issueCount, locale)
                  })}
                </>
              )}
            </button>
          </div>
        </div>
        {volume && (
          <CapacityStrip
            volume={volume}
            byteUnitScale={settings.byteUnitScale}
          />
        )}
      </div>

      {scan.kind === "authorizing" ? (
        progressScreen(t("authenticationRequired"), t("authenticationPrompt"))
      ) : scan.kind === "scanning" ? (
        progressScreen(t("readingFolders"))
      ) : scan.kind === "cancelling" ? (
        progressScreen(t("statusCancelling"))
      ) : scan.kind === "loading" ? (
        progressScreen(t("preparingMap"))
      ) : view ? (
        <main
          className={`scan-layout ${settings.showSidebar ? "" : "scan-layout--map-only"} ${
            selected || lens ? "scan-layout--details" : ""
          }`}
        >
          {settings.showSidebar && (
            <aside
              className="folder-sidebar"
              aria-label={t("filesAndFolders")}
            >
              <div className="sidebar-heading">
                <div>
                  <List size={17} />
                  <strong>{t("largestFirst")}</strong>
                </div>
                <span>
                  {t("itemsCount", {
                    count: formatCount(children.total, locale)
                  })}
                </span>
              </div>
              <div className="file-list" role="tree">
                {children.items.length === 0 ? (
                  <div className="empty-list">
                    <Folder size={27} weight="duotone" />
                    <strong>{t("noFilesShown")}</strong>
                    <span>{t("emptyOrExcluded")}</span>
                  </div>
                ) : (
                  children.items.map((node) => (
                    <div
                      className={`file-row ${
                        selectedId === node.id ? "file-row--selected" : ""
                      }`}
                      key={node.id}
                      role="treeitem"
                      tabIndex={0}
                      aria-selected={selectedId === node.id}
                      onClick={() => onSelect(node.id)}
                      onDoubleClick={() => {
                        if (node.kind === "directory") onCenter(node);
                        else void runAction("open", { ...node, uri: "", modifiedMs: null, permissions: null, hardLinks: 1 });
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          if (node.kind === "directory") onCenter(node);
                          else onSelect(node.id);
                        }
                      }}
                    >
                      <span className={`file-icon file-icon--${node.kind}`}>
                        {node.kind === "directory" ? (
                          <Folder size={19} weight="fill" />
                        ) : (
                          <span className="file-glyph" />
                        )}
                      </span>
                      <span className="file-row__name">
                        <strong title={node.name} dir="auto">
                          {node.kind === "small_files"
                            ? t("nodeSmallFiles")
                            : node.name}
                        </strong>
                        <small>
                          {node.kind === "directory"
                            ? t("filesCount", {
                                count: formatCount(node.fileCount, locale)
                              })
                            : node.kind === "file"
                              ? t("nodeFile")
                              : node.kind === "symlink"
                                ? t("nodeSymlink")
                                : node.kind === "small_files"
                                  ? t("nodeSmallFiles")
                                  : t("nodeOther")}
                        </small>
                      </span>
                      <span className="file-row__size">
                        {formatBytes(
                          node.allocatedBytes,
                          settings.byteUnitScale,
                          locale
                        )}
                      </span>
                      <ItemMenu
                        node={node}
                        onCenter={() => onCenter(node)}
                        onAction={async (action) => {
                          onSelect(node.id);
                          if (!scanId) return;
                          try {
                            if (action === "request_delete") {
                              const detail = await getNode(scanId, node.id);
                              setSelected(detail);
                              onRequestDelete(detail);
                            } else {
                              const output = await performFileAction(
                                scanId,
                                node.id,
                                action
                              );
                              if (action === "copy_path" && output) {
                                await writeText(output);
                                onNotify(t("pathCopied"));
                              } else if (action === "trash") {
                                onNotify(
                                  t("movedToTrash", { name: node.name })
                                );
                                onSelect(null);
                                onReload();
                              }
                            }
                          } catch (error) {
                            onNotify(String(error), "error");
                          }
                        }}
                      />
                    </div>
                  ))
                )}
                {children.items.length < children.total && (
                  <button className="load-more" onClick={onMoreChildren}>
                    {t("showMore", {
                      count: formatCount(
                        Math.min(240, children.total - children.items.length),
                        locale
                      )
                    })}
                  </button>
                )}
              </div>
            </aside>
          )}

          <section className="map-panel">
            <RadialMap
              root={view}
              lens={mapLens}
              scheme={settings.colorScheme}
              contrast={settings.contrast}
              byteUnitScale={settings.byteUnitScale}
              selectedId={selectedId}
              onSelect={(node) => onSelect(node.id)}
              onCenter={onCenter}
            />
            <div className="map-legend">
              {lens?.kind === "compare" ? (
                <>
                  <span>
                    <i style={{ background: GROW_COLOR }} /> + {t("cmpGrew")}
                  </span>
                  <span>
                    <i style={{ background: SHRINK_COLOR }} /> − {t("cmpShrank")}
                  </span>
                  <span>
                    <i className="legend-neutral" /> {t("cmpUnchanged")}
                  </span>
                  <button
                    className="legend-exit"
                    onClick={analysis?.onExitLens}
                  >
                    {t("leaveComparison")}
                  </button>
                </>
              ) : lens?.kind === "reclaim" ? (
                <>
                  <span>
                    <i className="legend-folder" /> {t("reclaimable")}
                  </span>
                  <span>
                    <i className="legend-neutral" /> {t("reclaimOther")}
                  </span>
                </>
              ) : (
                <>
                  <span>
                    <i className="legend-folder" /> {t("mapFolders")}
                  </span>
                  <span>
                    <i className="legend-file" /> {t("mapFilesGrouped")}
                  </span>
                  <small>{t("centerMapHint")}</small>
                </>
              )}
            </div>
          </section>

          {analysis && lens?.kind === "compare" && (
            <ComparePanel
              diff={lens.diff}
              scale={settings.byteUnitScale}
              onShow={(nodeId) => onCenter({ id: nodeId, kind: "directory" })}
              onExit={analysis.onExitLens}
            />
          )}
          {analysis && lens?.kind === "reclaim" && (
            <ReclaimPanel
              data={lens.data}
              scale={settings.byteUnitScale}
              onShow={(nodeId) => onCenter({ id: nodeId, kind: "directory" })}
              onExit={analysis.onExitLens}
              onNotify={onNotify}
            />
          )}

          {selected && !lens && (
            <aside className="detail-panel" aria-label={t("itemDetails")}>
              <div className="detail-heading">
                <span className={`detail-icon detail-icon--${selected.kind}`}>
                  {selected.kind === "directory" ? (
                    <Folder size={24} weight="fill" />
                  ) : (
                    <span className="file-glyph file-glyph--large" />
                  )}
                </span>
                <div>
                  <h2 dir="auto">{selected.name}</h2>
                  <p title={selected.displayPath} dir="auto">
                    {selected.displayPath}
                  </p>
                </div>
                <button
                  className="icon-button"
                  aria-label={t("closeDetails")}
                  onClick={() => onSelect(null)}
                >
                  <X size={18} />
                </button>
              </div>
              {detailLoading ? (
                <div className="detail-skeleton">
                  <span />
                  <span />
                  <span />
                </div>
              ) : (
                <>
                  <dl className="detail-stats">
                    <div>
                      <dt>{t("diskUsage")}</dt>
                      <dd>
                        {formatBytes(
                          selected.allocatedBytes,
                          settings.byteUnitScale,
                          locale
                        )}
                      </dd>
                    </div>
                    <div>
                      <dt>{t("apparentSize")}</dt>
                      <dd>
                        {formatBytes(
                          selected.apparentBytes,
                          settings.byteUnitScale,
                          locale
                        )}
                      </dd>
                    </div>
                    {selected.kind === "directory" && (
                      <>
                        <div>
                          <dt>{t("files")}</dt>
                          <dd>{formatCount(selected.fileCount, locale)}</dd>
                        </div>
                        <div>
                          <dt>{t("folders")}</dt>
                          <dd>
                            {formatCount(selected.directoryCount, locale)}
                          </dd>
                        </div>
                      </>
                    )}
                    {selected.permissions && (
                      <div>
                        <dt>{t("permissions")}</dt>
                        <dd className="mono" dir="ltr">
                          {selected.permissions}
                        </dd>
                      </div>
                    )}
                    {selected.hardLinks > 1 && (
                      <div>
                        <dt>{t("hardLinks")}</dt>
                        <dd>{formatCount(selected.hardLinks, locale)}</dd>
                      </div>
                    )}
                  </dl>
                  {selected.flags.includes("approximate") && (
                    <p className="detail-note">{t("approximateSizes")}</p>
                  )}
                  <div className="detail-actions">
                    <button
                      className="button button--primary button--full"
                      onClick={() =>
                        selected.kind === "directory"
                          ? onCenter(selected)
                          : void runAction("open")
                      }
                    >
                      {selected.kind === "directory" ? (
                        <ChartDonut size={17} />
                      ) : (
                        <FolderOpen size={17} />
                      )}
                      {selected.kind === "directory"
                        ? t("centerMap")
                        : t("open")}
                    </button>
                    <div className="detail-action-grid">
                      <button
                        className="button button--quiet-border"
                        onClick={() => void runAction("reveal")}
                      >
                        <Folder size={16} />
                        {t("fileManager")}
                      </button>
                      <button
                        className="button button--quiet-border"
                        onClick={() => void runAction("copy_path")}
                      >
                        <Clipboard size={16} />
                        {t("copyPath")}
                      </button>
                      {selected.uri.startsWith("file:") && (
                        <button
                          className="button button--quiet-border"
                          onClick={() => void runAction("terminal")}
                        >
                          <Terminal size={16} />
                          {t("terminal")}
                        </button>
                      )}
                      {selected.parentId !== null && (
                        <button
                          className="button button--quiet-border"
                          onClick={() => void runAction("trash")}
                        >
                          <Trash size={16} />
                          {t("trash")}
                        </button>
                      )}
                    </div>
                  </div>
                  {selected.parentId !== null && (
                    <button
                      className="danger-link"
                      onClick={() => onRequestDelete(selected)}
                    >
                      {t("permanentlyDelete")}
                    </button>
                  )}
                </>
              )}
            </aside>
          )}
        </main>
      ) : scan.kind === "failed" ? (
        <main className="failure-state">
          <span>
            <Warning size={28} weight="fill" />
          </span>
          <h1>{t("scanFailureTitle")}</h1>
          <p>{t("scanFailureDescription")}</p>
          {scan.message && (
            <p className="mono" dir="auto">
              {scan.message}
            </p>
          )}
          <div>
            <button className="button button--primary" onClick={onRescan}>
              {t("tryAgain")}
            </button>
            <button className="button button--quiet" onClick={onOverview}>
              {t("returnToOverview")}
            </button>
          </div>
        </main>
      ) : (
        <main className="failure-state">
          <span>
            <Info size={28} />
          </span>
          <h1>{t("emptyResultTitle")}</h1>
          <p>{t(partial ? "emptyResultPartial" : "emptyResultUnavailable")}</p>
          <div>
            <button className="button button--primary" onClick={onRescan}>
              {t("rescan")}
            </button>
            <button className="button button--quiet" onClick={onOverview}>
              {t("returnToOverview")}
            </button>
          </div>
        </main>
      )}
    </div>
  );
}
