use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    Standard,
    Administrator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub cross_filesystems: bool,
    pub include_remote_mounts: bool,
    pub include_removable: bool,
    pub show_small_files: bool,
    pub exclusions: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            cross_filesystems: false,
            include_remote_mounts: false,
            include_removable: true,
            show_small_files: false,
            exclusions: vec!["/proc".into(), "/sys".into(), "/dev".into(), "/run".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub target: String,
    pub mode: ScanMode,
    pub options: ScanOptions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Directory,
    File,
    Symlink,
    SmallFiles,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub display_path: String,
    pub uri: String,
    /// Raw bytes on the wire: Linux names need not be UTF-8, and trash/delete act on this path.
    #[serde(with = "path_bytes")]
    pub local_path: Option<PathBuf>,
    pub kind: NodeKind,
    pub allocated_bytes: u64,
    pub apparent_bytes: u64,
    pub children: Vec<u64>,
    pub file_count: u64,
    pub directory_count: u64,
    pub modified_ms: Option<u64>,
    pub permissions: Option<String>,
    pub hard_links: u64,
    pub flags: Vec<String>,
}

mod path_bytes {
    use serde::{Deserializer, Serializer, de};
    use std::ffi::OsString;
    use std::fmt;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;

    pub fn serialize<S: Serializer>(
        path: &Option<PathBuf>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match path {
            Some(path) => serializer.serialize_bytes(path.as_os_str().as_bytes()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PathBuf>, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Option<PathBuf>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a path as a byte string")
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(None)
            }
            fn visit_some<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                deserializer.deserialize_bytes(self)
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(Some(PathBuf::from(OsString::from_vec(bytes.to_vec()))))
            }
            fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<Self::Value, E> {
                Ok(Some(PathBuf::from(OsString::from_vec(bytes))))
            }
        }
        deserializer.deserialize_option(Visitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanTree {
    pub root_id: u64,
    pub nodes: Vec<ScanNode>,
    pub issues: Vec<ScanIssue>,
    pub summary: ScanSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSummary {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub display_path: String,
    pub kind: NodeKind,
    pub allocated_bytes: u64,
    pub apparent_bytes: u64,
    pub child_count: usize,
    pub file_count: u64,
    pub directory_count: u64,
    pub flags: Vec<String>,
}

impl From<&ScanNode> for NodeSummary {
    fn from(node: &ScanNode) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent_id,
            name: node.name.clone(),
            display_path: node.display_path.clone(),
            kind: node.kind,
            allocated_bytes: node.allocated_bytes,
            apparent_bytes: node.apparent_bytes,
            child_count: node.children.len(),
            file_count: node.file_count,
            directory_count: node.directory_count,
            flags: node.flags.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewNode {
    #[serde(flatten)]
    pub summary: NodeSummary,
    pub children: Vec<ViewNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    #[serde(flatten)]
    pub summary: NodeSummary,
    pub uri: String,
    pub modified_ms: Option<u64>,
    pub permissions: Option<String>,
    pub hard_links: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Idle,
    Authorizing,
    Scanning,
    Complete,
    CompleteWithIssues,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub bytes_scanned: u64,
    pub current_path: String,
    pub issues: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    PermissionDenied,
    Io,
    Excluded,
    FilesystemBoundary,
    Changed,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub path: String,
    pub kind: IssueKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub files: u64,
    pub directories: u64,
    pub allocated_bytes: u64,
    pub apparent_bytes: u64,
    pub issues: usize,
    pub excluded: usize,
    pub elapsed_ms: u64,
    pub status: ScanStatus,
    pub elevated: bool,
}

impl Default for ScanSummary {
    fn default() -> Self {
        Self {
            files: 0,
            directories: 0,
            allocated_bytes: 0,
            apparent_bytes: 0,
            issues: 0,
            excluded: 0,
            elapsed_ms: 0,
            status: ScanStatus::Idle,
            elevated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ScanProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<ScanIssue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ScanSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ScanEvent {
    pub fn authorizing(scan_id: String) -> Self {
        Self {
            event: "authorizing".into(),
            ..Self::started(scan_id)
        }
    }

    pub fn started(scan_id: String) -> Self {
        Self {
            event: "started".into(),
            scan_id: Some(scan_id),
            progress: None,
            issue: None,
            summary: None,
            message: None,
        }
    }

    pub fn progress(scan_id: String, progress: ScanProgress) -> Self {
        Self {
            event: "progress".into(),
            scan_id: Some(scan_id),
            progress: Some(progress),
            issue: None,
            summary: None,
            message: None,
        }
    }

    pub fn issue(scan_id: String, issue: ScanIssue) -> Self {
        Self {
            event: "issue".into(),
            scan_id: Some(scan_id),
            progress: None,
            issue: Some(issue),
            summary: None,
            message: None,
        }
    }

    pub fn completed(scan_id: String, summary: ScanSummary) -> Self {
        Self {
            event: "completed".into(),
            scan_id: Some(scan_id),
            progress: None,
            issue: None,
            summary: Some(summary),
            message: None,
        }
    }

    pub fn cancelled(scan_id: String, summary: Option<ScanSummary>) -> Self {
        Self {
            event: "cancelled".into(),
            scan_id: Some(scan_id),
            progress: None,
            issue: None,
            summary,
            message: None,
        }
    }

    pub fn failed(scan_id: String, message: impl Into<String>) -> Self {
        Self {
            event: "failed".into(),
            scan_id: Some(scan_id),
            progress: None,
            issue: None,
            summary: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
    pub remote: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    #[default]
    System,
    En,
    Tr,
    Es,
    It,
    Fr,
    De,
    Ru,
    Ar,
    Zh,
    Ja,
    Ko,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    System,
    Rainbow,
    HighContrast,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ByteUnitScale {
    #[default]
    Binary,
    Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: LanguagePreference,
    pub theme: ThemePreference,
    pub color_scheme: ColorScheme,
    pub byte_unit_scale: ByteUnitScale,
    pub contrast: u8,
    pub show_sidebar: bool,
    pub keep_history: bool,
    pub scan_options: ScanOptions,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LanguagePreference::System,
            theme: ThemePreference::System,
            color_scheme: ColorScheme::System,
            byte_unit_scale: ByteUnitScale::Binary,
            contrast: 72,
            show_sidebar: true,
            keep_history: true,
            scan_options: ScanOptions::default(),
        }
    }
}
