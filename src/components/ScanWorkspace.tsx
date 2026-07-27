import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Tooltip from "@radix-ui/react-tooltip";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  ArrowCounterClockwise,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  CaretRight,
  ChartDonut,
  Check,
  Clipboard,
  DotsThree,
  Folder,
  FolderOpen,
  Gear,
  House,
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
  NodeDetail,
  NodeSummary,
  ScanIssue,
  ScanMode,
  ScanProgress,
  ScanStatus,
  ScanSummary,
  Settings,
  VolumeInfo,
  ViewNode
} from "../types";
import { getNode, performFileAction } from "../lib/backend";
import { formatBytes, formatCount } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { AdminBadge, LiScanMark } from "./Brand";
import { DeleteDialog } from "./Dialogs";
import { RadialMap } from "./RadialMap";
import { CapacityStrip } from "./CapacityStrip";

interface ScanWorkspaceProps {
  scanId: string | null;
  target: string;
  mode: ScanMode;
  status: ScanStatus;
  progress: ScanProgress | null;
  summary: ScanSummary | null;
  view: ViewNode | null;
  children: NodeSummary[];
  selectedId: number | null;
  settings: Settings;
  volume: VolumeInfo | null;
  canGoBack: boolean;
  canGoForward: boolean;
  issues: ScanIssue[];
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
  scanId,
  target,
  mode,
  status,
  progress,
  summary,
  view,
  children,
  selectedId,
  settings,
  volume,
  canGoBack,
  canGoForward,
  issues,
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
  onViewDepth,
  onNotify
}: ScanWorkspaceProps) {
  const { locale, t } = useI18n();
  const [selected, setSelected] = useState<NodeDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [visibleRows, setVisibleRows] = useState(240);

  useEffect(() => {
    setVisibleRows(240);
  }, [view?.id]);

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

  const running = status === "scanning" || status === "authorizing";
  const visibleChildren = useMemo(
    () => children.slice(0, visibleRows),
    [children, visibleRows]
  );

  async function runAction(action: string, node = selected) {
    if (!scanId || !node) return;
    if (action === "request_delete") {
      setDeleteOpen(true);
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

  const statusLabel =
    status === "authorizing"
      ? t("statusWaitingAuthentication")
      : status === "scanning"
        ? t("statusScanning")
        : status === "complete_with_issues"
          ? t("statusCompletedWithIssues")
          : status === "cancelled"
            ? t("statusPartialScan")
            : status === "failed"
              ? t("statusScanFailed")
              : t("statusScanComplete");

  return (
    <div
      className={`workspace-shell ${
        volume ? "workspace-shell--capacity" : ""
      }`}
    >
      <header className="workspace-header">
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
        <div
          className="path-bar"
          title={view?.displayPath ?? target}
          dir="auto"
        >
          <Folder size={16} weight="fill" />
          <span>{view?.displayPath ?? target}</span>
        </div>
        <div className="workspace-tools">
          {running ? (
            <button className="button button--stop" onClick={onCancel}>
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
            {summary && !running && (
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
              ) : issues.length === 0 ? (
                <>
                  <Check size={14} /> {t("fullCoverage")}
                </>
              ) : (
                <>
                  <Info size={14} />{" "}
                  {t("reportsCount", {
                    count: formatCount(issues.length, locale)
                  })}
                </>
              )}
            </button>
          </div>
          {running && (
            <div className="scan-progress-track" aria-hidden="true">
              <span />
            </div>
          )}
        </div>
        {volume && (
          <CapacityStrip
            volume={volume}
            byteUnitScale={settings.byteUnitScale}
          />
        )}
      </div>

      {running && !view ? (
        <main className="scanning-state">
          <div className="scanning-visual" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <h1>
            {status === "authorizing"
              ? t("authenticationRequired")
              : t("readingFolders")}
          </h1>
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
              <strong>
                {formatCount(progress?.filesScanned ?? 0, locale)}
              </strong>
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
          <button className="button button--quiet-border" onClick={onCancel}>
            <X size={16} />
            {t("cancelScan")}
          </button>
        </main>
      ) : view ? (
        <main
          className={`scan-layout ${settings.showSidebar ? "" : "scan-layout--map-only"} ${
            selected ? "scan-layout--details" : ""
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
                    count: formatCount(children.length, locale)
                  })}
                </span>
              </div>
              <div className="file-list" role="tree">
                {visibleChildren.length === 0 ? (
                  <div className="empty-list">
                    <Folder size={27} weight="duotone" />
                    <strong>{t("noFilesShown")}</strong>
                    <span>{t("emptyOrExcluded")}</span>
                  </div>
                ) : (
                  visibleChildren.map((node) => (
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
                              setDeleteOpen(true);
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
                {children.length > visibleRows && (
                  <button
                    className="load-more"
                    onClick={() => setVisibleRows((count) => count + 240)}
                  >
                    {t("showMore", {
                      count: formatCount(
                        Math.min(240, children.length - visibleRows),
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
              scheme={settings.colorScheme}
              contrast={settings.contrast}
              byteUnitScale={settings.byteUnitScale}
              selectedId={selectedId}
              onSelect={(node) => onSelect(node.id)}
              onCenter={onCenter}
            />
            <div className="map-legend">
              <span>
                <i className="legend-folder" /> {t("mapFolders")}
              </span>
              <span>
                <i className="legend-file" /> {t("mapFilesGrouped")}
              </span>
              <small>{t("centerMapHint")}</small>
            </div>
          </section>

          {selected && (
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
                      onClick={() => setDeleteOpen(true)}
                    >
                      {t("permanentlyDelete")}
                    </button>
                  )}
                </>
              )}
            </aside>
          )}
        </main>
      ) : status === "failed" ? (
        <main className="failure-state">
          <span>
            <Warning size={28} weight="fill" />
          </span>
          <h1>{t("scanFailureTitle")}</h1>
          <p>{t("scanFailureDescription")}</p>
          <div>
            <button className="button button--primary" onClick={onRescan}>
              {t("tryAgain")}
            </button>
            <button className="button button--quiet" onClick={onOverview}>
              {t("returnToOverview")}
            </button>
          </div>
        </main>
      ) : null}

      <DeleteDialog
        node={selected}
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        onConfirm={async () => {
          try {
            if (!scanId || !selected) return;
            await performFileAction(scanId, selected.id, "delete");
            onNotify(t("permanentlyDeleted", { name: selected.name }));
            onSelect(null);
            onReload();
          } catch (error) {
            onNotify(String(error), "error");
          }
        }}
      />
    </div>
  );
}
