import * as Dialog from "@radix-ui/react-dialog";
import * as Slider from "@radix-ui/react-slider";
import * as Switch from "@radix-ui/react-switch";
import {
  Check,
  FolderOpen,
  Info,
  LockKey,
  ShieldCheck,
  Warning,
  X
} from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import type { NodeDetail, ScanIssue, Settings } from "../types";
import { chooseFolder } from "../lib/backend";
import { LiScanMark } from "./Brand";

function DialogFrame({
  title,
  description,
  icon,
  children,
  width = "normal"
}: {
  title: string;
  description?: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
  width?: "normal" | "wide";
}) {
  return (
    <Dialog.Portal>
      <Dialog.Overlay className="dialog-overlay" />
      <Dialog.Content
        className={`dialog-content ${width === "wide" ? "dialog-content--wide" : ""}`}
      >
        <div className="dialog-heading">
          {icon && <span className="dialog-heading__icon">{icon}</span>}
          <div>
            <Dialog.Title>{title}</Dialog.Title>
            {description && (
              <Dialog.Description>{description}</Dialog.Description>
            )}
          </div>
          <Dialog.Close className="icon-button dialog-close" aria-label="Close">
            <X size={18} />
          </Dialog.Close>
        </div>
        {children}
      </Dialog.Content>
    </Dialog.Portal>
  );
}

interface AdminDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultTarget: string;
  onStart: (target: string) => void;
}

export function AdminDialog({
  open,
  onOpenChange,
  defaultTarget,
  onStart
}: AdminDialogProps) {
  const [target, setTarget] = useState(defaultTarget || "/");
  useEffect(() => {
    if (open) setTarget(defaultTarget || "/");
  }, [defaultTarget, open]);

  async function browse() {
    const folder = await chooseFolder();
    if (folder) setTarget(folder);
  }

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title="Scan as administrator"
        description="Include protected files without running the LiScan interface as root."
        icon={<LockKey size={22} weight="duotone" />}
      >
        <div className="dialog-body">
          <label className="field">
            <span>Folder to scan</span>
            <span className="input-with-action">
              <input
                value={target}
                onChange={(event) => setTarget(event.target.value)}
                spellCheck={false}
              />
              <button className="icon-button" onClick={browse} aria-label="Browse">
                <FolderOpen size={19} />
              </button>
            </span>
          </label>

          <div className="safety-list">
            <div>
              <Check size={17} weight="bold" />
              <span>Reads metadata from protected local folders</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>Includes local filesystems and Btrfs subvolumes</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>Reports every skipped path and read error</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>Skips snapshot trees and duplicate container mount views</span>
            </div>
            <div className="safety-list__excluded">
              <Info size={17} />
              <span>Virtual filesystems such as /proc and /sys stay excluded</span>
            </div>
          </div>

          <div className="security-note">
            <ShieldCheck size={20} weight="duotone" />
            <span>
              The helper can inspect names and sizes only. It cannot delete,
              modify, execute, or upload files.
            </span>
          </div>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">Cancel</Dialog.Close>
          <button
            className="button button--primary"
            disabled={!target.trim().startsWith("/")}
            onClick={() => {
              onOpenChange(false);
              onStart(target.trim());
            }}
          >
            Authenticate and scan
          </button>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}

export function RemoteDialog({
  open,
  onOpenChange,
  onStart
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onStart: (uri: string) => void;
}) {
  const [uri, setUri] = useState("sftp://");
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title="Scan a remote location"
        description="Use a location supported by GIO and your installed GVfs backends."
      >
        <div className="dialog-body">
          <label className="field">
            <span>Remote URI</span>
            <input
              value={uri}
              onChange={(event) => setUri(event.target.value)}
              placeholder="sftp://server/home/user"
              spellCheck={false}
              autoFocus
            />
            <small>Examples: sftp://, smb://, ftp://</small>
          </label>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">Cancel</Dialog.Close>
          <button
            className="button button--primary"
            disabled={!uri.includes("://")}
            onClick={() => {
              onOpenChange(false);
              onStart(uri.trim());
            }}
          >
            Scan location
          </button>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}

function SettingSwitch({
  checked,
  onCheckedChange,
  label,
  description,
  disabled = false
}: {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  label: string;
  description: string;
  disabled?: boolean;
}) {
  return (
    <div className={`setting-row ${disabled ? "setting-row--disabled" : ""}`}>
      <div>
        <strong>{label}</strong>
        <small>{description}</small>
      </div>
      <Switch.Root
        className="switch"
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        aria-label={label}
      >
        <Switch.Thumb className="switch__thumb" />
      </Switch.Root>
    </div>
  );
}

export function SettingsDialog({
  open,
  onOpenChange,
  settings,
  onSave
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  settings: Settings;
  onSave: (settings: Settings) => void;
}) {
  const [draft, setDraft] = useState(settings);
  useEffect(() => {
    if (open) setDraft(structuredClone(settings));
  }, [open, settings]);

  const updateOptions = (next: Partial<Settings["scanOptions"]>) =>
    setDraft((current) => ({
      ...current,
      scanOptions: { ...current.scanOptions, ...next }
    }));

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title="Settings"
        description="Choose how LiScan traverses and displays your files."
        width="wide"
      >
        <div className="settings-grid">
          <section>
            <h3>Scanning</h3>
            <SettingSwitch
              checked={draft.scanOptions.crossFilesystems}
              onCheckedChange={(crossFilesystems) =>
                updateOptions({ crossFilesystems })
              }
              label="Cross filesystem boundaries"
              description="Continue into other mounted filesystems below the selected folder."
            />
            <SettingSwitch
              checked={draft.scanOptions.includeRemoteMounts}
              onCheckedChange={(includeRemoteMounts) =>
                updateOptions({ includeRemoteMounts })
              }
              label="Include remote mounts"
              description="May transfer a large amount of network metadata."
              disabled={!draft.scanOptions.crossFilesystems}
            />
            <SettingSwitch
              checked={draft.scanOptions.includeRemovable}
              onCheckedChange={(includeRemovable) =>
                updateOptions({ includeRemovable })
              }
              label="Include removable storage"
              description="Scan connected USB drives and other removable media."
              disabled={!draft.scanOptions.crossFilesystems}
            />
            <div className="exclusion-editor">
              <div>
                <strong>Excluded folders</strong>
                <small>One absolute path per line.</small>
              </div>
              <textarea
                value={draft.scanOptions.exclusions.join("\n")}
                onChange={(event) =>
                  updateOptions({
                    exclusions: event.target.value
                      .split("\n")
                      .map((value) => value.trim())
                      .filter(Boolean)
                  })
                }
                rows={5}
                spellCheck={false}
              />
            </div>
          </section>

          <section>
            <h3>Appearance</h3>
            <label className="field">
              <span>Theme</span>
              <select
                value={draft.theme}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    theme: event.target.value as Settings["theme"]
                  }))
                }
              >
                <option value="system">Follow system</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </label>
            <label className="field">
              <span>Size units</span>
              <select
                value={draft.byteUnitScale}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    byteUnitScale: event.target
                      .value as Settings["byteUnitScale"]
                  }))
                }
              >
                <option value="binary">
                  Binary — KiB, MiB, GiB (1024)
                </option>
                <option value="decimal">
                  Decimal — kB, MB, GB (1000)
                </option>
              </select>
            </label>
            <label className="field">
              <span>Map colors</span>
              <select
                value={draft.colorScheme}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    colorScheme: event.target.value as Settings["colorScheme"]
                  }))
                }
              >
                <option value="system">System green</option>
                <option value="rainbow">Categorical</option>
                <option value="high_contrast">High contrast</option>
              </select>
            </label>
            <div className="slider-setting">
              <div>
                <strong>Map contrast</strong>
                <small>{draft.contrast}%</small>
              </div>
              <Slider.Root
                className="slider"
                min={20}
                max={100}
                step={1}
                value={[draft.contrast]}
                onValueChange={([contrast]) =>
                  setDraft((current) => ({ ...current, contrast }))
                }
              >
                <Slider.Track className="slider__track">
                  <Slider.Range className="slider__range" />
                </Slider.Track>
                <Slider.Thumb className="slider__thumb" aria-label="Map contrast" />
              </Slider.Root>
            </div>
            <SettingSwitch
              checked={draft.showSidebar}
              onCheckedChange={(showSidebar) =>
                setDraft((current) => ({ ...current, showSidebar }))
              }
              label="Folder sidebar"
              description="Show a sortable text alternative beside the radial map."
            />
            <SettingSwitch
              checked={draft.scanOptions.showSmallFiles}
              onCheckedChange={(showSmallFiles) =>
                updateOptions({ showSmallFiles })
              }
              label="Show more small files"
              description="Render more individual map segments instead of grouping them."
            />
          </section>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">Cancel</Dialog.Close>
          <button
            className="button button--primary"
            onClick={() => {
              onSave(draft);
              onOpenChange(false);
            }}
          >
            Save settings
          </button>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}

export function DeleteDialog({
  node,
  open,
  onOpenChange,
  onConfirm
}: {
  node: NodeDetail | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  const [confirmation, setConfirmation] = useState("");
  useEffect(() => {
    if (open) setConfirmation("");
  }, [open]);
  if (!node) return null;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title="Permanently delete item?"
        description="This cannot be undone or recovered from Trash."
        icon={<Warning size={22} weight="fill" />}
      >
        <div className="dialog-body">
          <div className="delete-target">
            <strong>{node.name}</strong>
            <span>{node.displayPath}</span>
          </div>
          <label className="field">
            <span>
              Type <strong>{node.name}</strong> to confirm
            </span>
            <input
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
              autoFocus
            />
          </label>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">Cancel</Dialog.Close>
          <button
            className="button button--danger"
            disabled={confirmation !== node.name}
            onClick={() => {
              onConfirm();
              onOpenChange(false);
            }}
          >
            Permanently delete
          </button>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}

export function CoverageDialog({
  open,
  onOpenChange,
  issues
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  issues: ScanIssue[];
}) {
  const failures = issues.filter(
    (issue) =>
      issue.kind !== "excluded" && issue.kind !== "filesystem_boundary"
  );
  const exclusions = issues.length - failures.length;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title="Scan coverage"
        description="Every path LiScan could not traverse is listed here."
        width="wide"
      >
        <div className="coverage-summary">
          <div>
            <strong>{failures.length}</strong>
            <span>Read issues</span>
          </div>
          <div>
            <strong>{exclusions}</strong>
            <span>Policy exclusions</span>
          </div>
        </div>
        <div className="coverage-list">
          {issues.length === 0 ? (
            <div className="coverage-clear">
              <Check size={21} weight="bold" />
              Every reachable entry inside the selected boundaries was scanned.
            </div>
          ) : (
            issues.map((issue, index) => (
              <div className="coverage-row" key={`${issue.path}-${index}`}>
                <span
                  className={`coverage-row__icon coverage-row__icon--${issue.kind}`}
                >
                  {issue.kind === "excluded" ||
                  issue.kind === "filesystem_boundary" ? (
                    <Info size={16} />
                  ) : (
                    <Warning size={16} />
                  )}
                </span>
                <span>
                  <strong>{issue.path}</strong>
                  <small>{issue.message}</small>
                </span>
              </div>
            ))
          )}
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--primary">Done</Dialog.Close>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}

export function AboutDialog({
  open,
  onOpenChange
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame title="About LiScan">
        <div className="about-content">
          <LiScanMark />
          <p>
            A clear, local-first disk usage scanner for Linux with safely
            bounded administrator access.
          </p>
          <dl>
            <div>
              <dt>Version</dt>
              <dd>0.1.0</dd>
            </div>
            <div>
              <dt>License</dt>
              <dd>GPL-3.0-or-later</dd>
            </div>
          </dl>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--primary">Done</Dialog.Close>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}
