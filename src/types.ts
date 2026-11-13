export type ScanMode = "standard" | "administrator";
export type ScanStatus =
  | "idle"
  | "authorizing"
  | "scanning"
  | "complete"
  | "complete_with_issues"
  | "cancelled"
  | "failed";

export type NodeKind =
  | "directory"
  | "file"
  | "symlink"
  | "small_files"
  | "other";

export type ColorScheme = "system" | "rainbow" | "high_contrast";
export type ThemePreference = "system" | "light" | "dark";
export type ByteUnitScale = "binary" | "decimal";
export type AppLocale =
  | "en"
  | "tr"
  | "es"
  | "it"
  | "fr"
  | "de"
  | "ru"
  | "ar"
  | "zh"
  | "ja"
  | "ko";
export type LanguagePreference = "system" | AppLocale;

export interface ScanOptions {
  crossFilesystems: boolean;
  includeRemoteMounts: boolean;
  includeRemovable: boolean;
  showSmallFiles: boolean;
  exclusions: string[];
}

export interface ScanRequest {
  target: string;
  mode: ScanMode;
  options: ScanOptions;
}

export interface ScanProgress {
  filesScanned: number;
  directoriesScanned: number;
  bytesScanned: number;
  currentPath: string;
  issues: number;
}

export interface ScanIssue {
  path: string;
  kind:
    | "permission_denied"
    | "io"
    | "excluded"
    | "filesystem_boundary"
    | "changed"
    | "unsupported";
  message: string;
}

export interface ScanSummary {
  files: number;
  directories: number;
  allocatedBytes: number;
  apparentBytes: number;
  issues: number;
  excluded: number;
  elapsedMs: number;
  status: ScanStatus;
  elevated: boolean;
}

export interface ScanEvent {
  event:
    | "authorizing"
    | "started"
    | "progress"
    | "issue"
    | "completed"
    | "cancelled"
    | "failed";
  scanId?: string;
  progress?: ScanProgress;
  issue?: ScanIssue;
  summary?: ScanSummary;
  message?: string;
}

export interface NodeSummary {
  id: number;
  parentId: number | null;
  name: string;
  displayPath: string;
  kind: NodeKind;
  allocatedBytes: number;
  apparentBytes: number;
  childCount: number;
  fileCount: number;
  directoryCount: number;
  flags: string[];
}

export interface ViewNode extends NodeSummary {
  children: ViewNode[];
}

export interface NodeDetail extends NodeSummary {
  uri: string;
  modifiedMs: number | null;
  permissions: string | null;
  hardLinks: number;
}

export interface VolumeInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  totalBytes: number;
  availableBytes: number;
  removable: boolean;
  remote: boolean;
}

export interface Settings {
  language: LanguagePreference;
  theme: ThemePreference;
  colorScheme: ColorScheme;
  byteUnitScale: ByteUnitScale;
  contrast: number;
  showSidebar: boolean;
  keepHistory: boolean;
  scanOptions: ScanOptions;
}

export interface AppError {
  title: string;
  detail: string;
}

export interface SnapshotInfo {
  /** Unix seconds; also the snapshot's id. */
  timestamp: number;
  total: number;
}

export interface DiffRow {
  path: string;
  before: number;
  after: number;
  delta: number;
  /** Folder of the current scan, null when it no longer exists. */
  nodeId: number | null;
}

export interface ScanDiff {
  beforeTimestamp: number;
  beforeTotal: number;
  afterTotal: number;
  delta: number;
  rows: DiffRow[];
}

export interface ReclaimMatch {
  ruleId: string;
  /** Message keys owned by the interface (lib/i18n.tsx). */
  titleKey: string;
  explanationKey: string;
  command: string | null;
  warning: boolean;
  /** False when the real saving is unknowable; not part of the total. */
  counted: boolean;
  path: string;
  sizeBytes: number;
  nodeId: number;
}

export interface Reclaimable {
  totalBytes: number;
  matches: ReclaimMatch[];
  /** Matches left out of the list by the backend's cap. */
  more: number;
}

/** What a finished scan is currently being viewed through, if anything. */
export type Lens =
  | { kind: "compare"; diff: ScanDiff }
  | { kind: "reclaim"; data: Reclaimable };
