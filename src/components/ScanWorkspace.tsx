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
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button className="icon-button row-menu-button" aria-label="Item actions">
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
            Open
          </DropdownMenu.Item>
          {node.kind === "directory" && (
            <DropdownMenu.Item className="menu-item" onSelect={onCenter}>
              <ChartDonut size={17} />
              Center map here
            </DropdownMenu.Item>
          )}
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("reveal")}
          >
            <Folder size={17} />
            Open in file manager
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("terminal")}
          >
            <Terminal size={17} />
            Open terminal here
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("copy_path")}
          >
            <Clipboard size={17} />
            Copy path
          </DropdownMenu.Item>
          <DropdownMenu.Separator className="menu-separator" />
          <DropdownMenu.Item
            className="menu-item"
            onSelect={() => onAction("trash")}
          >
            <Trash size={17} />
            Move to Trash
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className="menu-item menu-item--danger"
            onSelect={() => onAction("request_delete")}
          >
            <Warning size={17} />
            Permanently delete
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
        onNotify("Path copied");
      } else if (action === "trash") {
        onNotify(`${node.name} moved to Trash`);
        onSelect(null);
        onReload();
      }
    } catch (error) {
      onNotify(String(error), "error");
    }
  }

  const statusLabel =
    status === "authorizing"
      ? "Waiting for administrator authentication"
      : status === "scanning"
        ? "Scanning"
        : status === "complete_with_issues"
          ? "Completed with issues"
          : status === "cancelled"
            ? "Partial scan"
            : status === "failed"
              ? "Scan failed"
              : "Scan complete";

  return (
    <div
      className={`workspace-shell ${
        volume ? "workspace-shell--capacity" : ""
      }`}
    >
      <header className="workspace-header">
        <button className="brand-button" onClick={onOverview} aria-label="Overview">
          <LiScanMark compact />
        </button>
        <div className="workspace-nav">
          <ToolbarButton label="Back" disabled={!canGoBack} onClick={onBack}>
            <ArrowLeft size={18} />
          </ToolbarButton>
          <ToolbarButton
            label="Forward"
            disabled={!canGoForward}
            onClick={onForward}
          >
            <ArrowRight size={18} />
          </ToolbarButton>
          <ToolbarButton
            label="Parent folder"
            disabled={!view?.parentId || running}
            onClick={onUp}
          >
            <ArrowUp size={18} />
          </ToolbarButton>
        </div>
        <div className="path-bar" title={view?.displayPath ?? target}>
          <Folder size={16} weight="fill" />
          <span>{view?.displayPath ?? target}</span>
        </div>
        <div className="workspace-tools">
          {running ? (
            <button className="button button--stop" onClick={onCancel}>
              <Stop size={16} weight="fill" />
              Stop
            </button>
          ) : (
            <ToolbarButton label="Rescan" onClick={onRescan}>
              <ArrowCounterClockwise size={18} />
            </ToolbarButton>
          )}
          <ToolbarButton label="Zoom in" onClick={() => onViewDepth(Math.max(2, viewDepth - 1))}>
            <MagnifyingGlassPlus size={18} />
          </ToolbarButton>
          <ToolbarButton label="Zoom out" onClick={() => onViewDepth(Math.min(6, viewDepth + 1))}>
            <MagnifyingGlassMinus size={18} />
          </ToolbarButton>
          <ToolbarButton label="Settings" onClick={onSettings}>
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
                <span>{formatCount(progress.filesScanned)} files</span>
                <span>
                  {formatBytes(progress.bytesScanned, settings.byteUnitScale)}
                </span>
              </>
            )}
            {summary && !running && (
              <>
                <span>{formatCount(summary.files)} files</span>
                <span>
                  {formatBytes(summary.allocatedBytes, settings.byteUnitScale)}
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
                  <Info size={14} /> Coverage pending
                </>
              ) : status === "failed" ? (
                <>
                  <Warning size={14} /> Coverage unavailable
                </>
              ) : status === "cancelled" ? (
                <>
                  <Info size={14} /> Partial coverage
                </>
              ) : issues.length === 0 ? (
                <>
                  <Check size={14} /> Full coverage
                </>
              ) : (
                <>
                  <Info size={14} /> {issues.length} reports
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
          <h1>{status === "authorizing" ? "Authentication required" : "Reading folders"}</h1>
          <p className="scanning-path">
            {progress?.currentPath || target}
          </p>
          <div className="scanning-stats">
            <div>
              <strong>{formatCount(progress?.directoriesScanned ?? 0)}</strong>
              <span>Folders</span>
            </div>
            <div>
              <strong>{formatCount(progress?.filesScanned ?? 0)}</strong>
              <span>Files</span>
            </div>
            <div>
              <strong>
                {formatBytes(
                  progress?.bytesScanned ?? 0,
                  settings.byteUnitScale
                )}
              </strong>
              <span>Counted</span>
            </div>
          </div>
          <button className="button button--quiet-border" onClick={onCancel}>
            <X size={16} />
            Cancel scan
          </button>
        </main>
      ) : view ? (
        <main
          className={`scan-layout ${settings.showSidebar ? "" : "scan-layout--map-only"} ${
            selected ? "scan-layout--details" : ""
          }`}
        >
          {settings.showSidebar && (
            <aside className="folder-sidebar" aria-label="Files and folders">
              <div className="sidebar-heading">
                <div>
                  <List size={17} />
                  <strong>Largest first</strong>
                </div>
                <span>{formatCount(children.length)} items</span>
              </div>
              <div className="file-list" role="tree">
                {visibleChildren.length === 0 ? (
                  <div className="empty-list">
                    <Folder size={27} weight="duotone" />
                    <strong>No files shown</strong>
                    <span>This folder is empty or entirely excluded.</span>
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
                        <strong title={node.name}>{node.name}</strong>
                        <small>
                          {node.kind === "directory"
                            ? `${formatCount(node.fileCount)} files`
                            : node.kind}
                        </small>
                      </span>
                      <span className="file-row__size">
                        {formatBytes(
                          node.allocatedBytes,
                          settings.byteUnitScale
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
                                onNotify("Path copied");
                              } else if (action === "trash") {
                                onNotify(`${node.name} moved to Trash`);
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
                    Show {Math.min(240, children.length - visibleRows)} more
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
                <i className="legend-folder" /> Folders
              </span>
              <span>
                <i className="legend-file" /> Files and grouped items
              </span>
              <small>Click a folder to center the map</small>
            </div>
          </section>

          {selected && (
            <aside className="detail-panel" aria-label="Item details">
              <div className="detail-heading">
                <span className={`detail-icon detail-icon--${selected.kind}`}>
                  {selected.kind === "directory" ? (
                    <Folder size={24} weight="fill" />
                  ) : (
                    <span className="file-glyph file-glyph--large" />
                  )}
                </span>
                <div>
                  <h2>{selected.name}</h2>
                  <p title={selected.displayPath}>{selected.displayPath}</p>
                </div>
                <button
                  className="icon-button"
                  aria-label="Close details"
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
                      <dt>Disk usage</dt>
                      <dd>
                        {formatBytes(
                          selected.allocatedBytes,
                          settings.byteUnitScale
                        )}
                      </dd>
                    </div>
                    <div>
                      <dt>Apparent size</dt>
                      <dd>
                        {formatBytes(
                          selected.apparentBytes,
                          settings.byteUnitScale
                        )}
                      </dd>
                    </div>
                    {selected.kind === "directory" && (
                      <>
                        <div>
                          <dt>Files</dt>
                          <dd>{formatCount(selected.fileCount)}</dd>
                        </div>
                        <div>
                          <dt>Folders</dt>
                          <dd>{formatCount(selected.directoryCount)}</dd>
                        </div>
                      </>
                    )}
                    {selected.permissions && (
                      <div>
                        <dt>Permissions</dt>
                        <dd className="mono">{selected.permissions}</dd>
                      </div>
                    )}
                    {selected.hardLinks > 1 && (
                      <div>
                        <dt>Hard links</dt>
                        <dd>{selected.hardLinks}</dd>
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
                      {selected.kind === "directory" ? "Center map" : "Open"}
                    </button>
                    <div className="detail-action-grid">
                      <button
                        className="button button--quiet-border"
                        onClick={() => void runAction("reveal")}
                      >
                        <Folder size={16} />
                        File manager
                      </button>
                      <button
                        className="button button--quiet-border"
                        onClick={() => void runAction("copy_path")}
                      >
                        <Clipboard size={16} />
                        Copy path
                      </button>
                      {selected.uri.startsWith("file:") && (
                        <button
                          className="button button--quiet-border"
                          onClick={() => void runAction("terminal")}
                        >
                          <Terminal size={16} />
                          Terminal
                        </button>
                      )}
                      {selected.parentId !== null && (
                        <button
                          className="button button--quiet-border"
                          onClick={() => void runAction("trash")}
                        >
                          <Trash size={16} />
                          Trash
                        </button>
                      )}
                    </div>
                  </div>
                  {selected.parentId !== null && (
                    <button
                      className="danger-link"
                      onClick={() => setDeleteOpen(true)}
                    >
                      Permanently delete
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
          <h1>LiScan could not complete this scan</h1>
          <p>
            Check that the location still exists and that the required
            filesystem backend is installed.
          </p>
          <div>
            <button className="button button--primary" onClick={onRescan}>
              Try again
            </button>
            <button className="button button--quiet" onClick={onOverview}>
              Return to overview
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
            onNotify(`${selected.name} permanently deleted`);
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
