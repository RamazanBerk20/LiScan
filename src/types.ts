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
  event: "started" | "progress" | "issue" | "completed" | "cancelled" | "failed";
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
  theme: ThemePreference;
  colorScheme: ColorScheme;
  byteUnitScale: ByteUnitScale;
  contrast: number;
  showSidebar: boolean;
  scanOptions: ScanOptions;
}

export interface AppError {
  title: string;
  detail: string;
}
