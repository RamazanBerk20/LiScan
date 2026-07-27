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
import type { AppLocale, NodeDetail, ScanIssue, Settings } from "../types";
import { chooseFolder } from "../lib/backend";
import {
  issueLabel,
  languageOptions,
  useI18n
} from "../lib/i18n";
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
  const { t } = useI18n();
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
          <Dialog.Close
            className="icon-button dialog-close"
            aria-label={t("close")}
          >
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
  const { t } = useI18n();
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
        title={t("adminTitle")}
        description={t("adminDescription")}
        icon={<LockKey size={22} weight="duotone" />}
      >
        <div className="dialog-body">
          <label className="field">
            <span>{t("folderToScan")}</span>
            <span className="input-with-action">
              <input
                value={target}
                onChange={(event) => setTarget(event.target.value)}
                spellCheck={false}
                dir="auto"
              />
              <button
                className="icon-button"
                onClick={browse}
                aria-label={t("browse")}
              >
                <FolderOpen size={19} />
              </button>
            </span>
          </label>

          <div className="safety-list">
            <div>
              <Check size={17} weight="bold" />
              <span>{t("adminReadsProtected")}</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>{t("adminIncludesFilesystems")}</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>{t("adminReportsSkipped")}</span>
            </div>
            <div>
              <Check size={17} weight="bold" />
              <span>{t("adminSkipsDuplicates")}</span>
            </div>
            <div className="safety-list__excluded">
              <Info size={17} />
              <span>{t("adminExcludesVirtual")}</span>
            </div>
          </div>

          <div className="security-note">
            <ShieldCheck size={20} weight="duotone" />
            <span>{t("adminSecurity")}</span>
          </div>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">
            {t("cancel")}
          </Dialog.Close>
          <button
            className="button button--primary"
            disabled={!target.trim().startsWith("/")}
            onClick={() => {
              onOpenChange(false);
              onStart(target.trim());
            }}
          >
            {t("authenticateAndScan")}
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
  const { t } = useI18n();
  const [uri, setUri] = useState("sftp://");
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title={t("remoteTitle")}
        description={t("remoteDescription")}
      >
        <div className="dialog-body">
          <label className="field">
            <span>{t("remoteUri")}</span>
            <input
              value={uri}
              onChange={(event) => setUri(event.target.value)}
              placeholder="sftp://server/home/user"
              spellCheck={false}
              dir="ltr"
              autoFocus
            />
            <small>{t("remoteExamples")}</small>
          </label>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">
            {t("cancel")}
          </Dialog.Close>
          <button
            className="button button--primary"
            disabled={!uri.includes("://")}
            onClick={() => {
              onOpenChange(false);
              onStart(uri.trim());
            }}
          >
            {t("scanLocation")}
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
  systemLocale,
  onSave
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  settings: Settings;
  systemLocale: AppLocale;
  onSave: (settings: Settings) => void;
}) {
  const { locale, t } = useI18n();
  const [draft, setDraft] = useState(settings);
  const systemLanguageName =
    languageOptions.find((option) => option.value === systemLocale)?.label ??
    "English";
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
        title={t("settingsTitle")}
        description={t("settingsDescription")}
        width="wide"
      >
        <div className="settings-grid">
          <section>
            <h3>{t("scanning")}</h3>
            <SettingSwitch
              checked={draft.scanOptions.crossFilesystems}
              onCheckedChange={(crossFilesystems) =>
                updateOptions({ crossFilesystems })
              }
              label={t("crossFilesystems")}
              description={t("crossFilesystemsDescription")}
            />
            <SettingSwitch
              checked={draft.scanOptions.includeRemoteMounts}
              onCheckedChange={(includeRemoteMounts) =>
                updateOptions({ includeRemoteMounts })
              }
              label={t("includeRemoteMounts")}
              description={t("includeRemoteMountsDescription")}
              disabled={!draft.scanOptions.crossFilesystems}
            />
            <SettingSwitch
              checked={draft.scanOptions.includeRemovable}
              onCheckedChange={(includeRemovable) =>
                updateOptions({ includeRemovable })
              }
              label={t("includeRemovable")}
              description={t("includeRemovableDescription")}
              disabled={!draft.scanOptions.crossFilesystems}
            />
            <div className="exclusion-editor">
              <div>
                <strong>{t("excludedFolders")}</strong>
                <small>{t("excludedFoldersDescription")}</small>
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
                dir="ltr"
              />
            </div>
          </section>

          <section>
            <h3>{t("appearance")}</h3>
            <label className="field">
              <span>{t("language")}</span>
              <select
                value={draft.language}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    language: event.target.value as Settings["language"]
                  }))
                }
              >
                <option value="system">
                  {t("systemLanguage")} — {systemLanguageName}
                </option>
                {languageOptions.map((option) => (
                  <option value={option.value} key={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("theme")}</span>
              <select
                value={draft.theme}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    theme: event.target.value as Settings["theme"]
                  }))
                }
              >
                <option value="system">{t("followSystem")}</option>
                <option value="light">{t("light")}</option>
                <option value="dark">{t("dark")}</option>
              </select>
            </label>
            <label className="field">
              <span>{t("sizeUnits")}</span>
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
                <option value="binary">{t("binaryUnits")}</option>
                <option value="decimal">{t("decimalUnits")}</option>
              </select>
            </label>
            <label className="field">
              <span>{t("mapColors")}</span>
              <select
                value={draft.colorScheme}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    colorScheme: event.target.value as Settings["colorScheme"]
                  }))
                }
              >
                <option value="system">{t("systemGreen")}</option>
                <option value="rainbow">{t("categorical")}</option>
                <option value="high_contrast">{t("highContrast")}</option>
              </select>
            </label>
            <div className="slider-setting">
              <div>
                <strong>{t("mapContrast")}</strong>
                <small>{draft.contrast}%</small>
              </div>
              <Slider.Root
                className="slider"
                dir={locale === "ar" ? "rtl" : "ltr"}
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
                <Slider.Thumb
                  className="slider__thumb"
                  aria-label={t("mapContrast")}
                />
              </Slider.Root>
            </div>
            <SettingSwitch
              checked={draft.showSidebar}
              onCheckedChange={(showSidebar) =>
                setDraft((current) => ({ ...current, showSidebar }))
              }
              label={t("folderSidebar")}
              description={t("folderSidebarDescription")}
            />
            <SettingSwitch
              checked={draft.scanOptions.showSmallFiles}
              onCheckedChange={(showSmallFiles) =>
                updateOptions({ showSmallFiles })
              }
              label={t("showSmallFiles")}
              description={t("showSmallFilesDescription")}
            />
          </section>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">
            {t("cancel")}
          </Dialog.Close>
          <button
            className="button button--primary"
            onClick={() => {
              onSave(draft);
              onOpenChange(false);
            }}
          >
            {t("saveSettings")}
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
  const { t } = useI18n();
  const [confirmation, setConfirmation] = useState("");
  useEffect(() => {
    if (open) setConfirmation("");
  }, [open]);
  if (!node) return null;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title={t("deleteTitle")}
        description={t("deleteDescription")}
        icon={<Warning size={22} weight="fill" />}
      >
        <div className="dialog-body">
          <div className="delete-target">
            <strong>{node.name}</strong>
            <span dir="auto">{node.displayPath}</span>
          </div>
          <label className="field">
            <span>{t("typeToConfirm", { name: node.name })}</span>
            <input
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
              autoFocus
              dir="auto"
            />
          </label>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--quiet">
            {t("cancel")}
          </Dialog.Close>
          <button
            className="button button--danger"
            disabled={confirmation !== node.name}
            onClick={() => {
              onConfirm();
              onOpenChange(false);
            }}
          >
            {t("permanentlyDelete")}
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
  const { t } = useI18n();
  const failures = issues.filter(
    (issue) =>
      issue.kind !== "excluded" && issue.kind !== "filesystem_boundary"
  );
  const exclusions = issues.length - failures.length;
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame
        title={t("coverageTitle")}
        description={t("coverageDescription")}
        width="wide"
      >
        <div className="coverage-summary">
          <div>
            <strong>{failures.length}</strong>
            <span>{t("readIssues")}</span>
          </div>
          <div>
            <strong>{exclusions}</strong>
            <span>{t("policyExclusions")}</span>
          </div>
        </div>
        <div className="coverage-list">
          {issues.length === 0 ? (
            <div className="coverage-clear">
              <Check size={21} weight="bold" />
              {t("coverageClear")}
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
                  <strong dir="auto">{issue.path}</strong>
                  <small title={issue.message}>{issueLabel(issue, t)}</small>
                </span>
              </div>
            ))
          )}
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--primary">
            {t("done")}
          </Dialog.Close>
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
  const { t } = useI18n();
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <DialogFrame title={t("aboutTitle")}>
        <div className="about-content">
          <LiScanMark />
          <p>{t("aboutDescription")}</p>
          <dl>
            <div>
              <dt>{t("version")}</dt>
              <dd>1.0.0</dd>
            </div>
            <div>
              <dt>{t("license")}</dt>
              <dd>GPL-3.0-or-later</dd>
            </div>
          </dl>
        </div>
        <div className="dialog-actions">
          <Dialog.Close className="button button--primary">
            {t("done")}
          </Dialog.Close>
        </div>
      </DialogFrame>
    </Dialog.Root>
  );
}
