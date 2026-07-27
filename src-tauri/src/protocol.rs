use crate::model::{ScanIssue, ScanNode, ScanProgress, ScanRequest, ScanSummary, ScanTree};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u16 = 2;
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;
const TREE_NODE_CHUNK_SIZE: usize = 256;
const TREE_ISSUE_CHUNK_SIZE: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminControl {
    Start { version: u16, request: ScanRequest },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminMessage {
    Started,
    Progress(ScanProgress),
    Issue(ScanIssue),
    TreeStart {
        root_id: u64,
        summary: ScanSummary,
        node_count: u64,
        issue_count: u64,
    },
    TreeNodes(Vec<ScanNode>),
    TreeIssues(Vec<ScanIssue>),
    TreeEnd,
    Cancelled,
    Failed(String),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum AdminMessageChunk<'a> {
    TreeNodes(&'a [ScanNode]),
    TreeIssues(&'a [ScanIssue]),
}

pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let mut data = Vec::new();
    ciborium::into_writer(value, &mut data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if data.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol frame exceeds size limit",
        ));
    }
    writer.write_all(&(data.len() as u64).to_be_bytes())?;
    writer.write_all(&data)?;
    writer.flush()
}

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut size = [0_u8; 8];
    reader.read_exact(&mut size)?;
    let size = u64::from_be_bytes(size) as usize;
    if size > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protocol frame exceeds size limit",
        ));
    }
    let mut data = vec![0_u8; size];
    reader.read_exact(&mut data)?;
    ciborium::from_reader(data.as_slice())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

pub fn write_scan_tree(writer: &mut impl Write, mut tree: ScanTree) -> io::Result<()> {
    let node_count = u64::try_from(tree.nodes.len())
        .map_err(|_| invalid_protocol("scan tree contains too many nodes"))?;
    let issue_count = u64::try_from(tree.issues.len())
        .map_err(|_| invalid_protocol("scan tree contains too many issues"))?;

    write_frame(
        writer,
        &AdminMessage::TreeStart {
            root_id: tree.root_id,
            summary: tree.summary,
            node_count,
            issue_count,
        },
    )?;

    // Parent IDs are sufficient to reconstruct this index. Omitting the
    // duplicated child lists keeps transfer memory bounded for very large
    // directory trees.
    for node in &mut tree.nodes {
        node.children = Vec::new();
    }
    for nodes in tree.nodes.chunks(TREE_NODE_CHUNK_SIZE) {
        write_frame(writer, &AdminMessageChunk::TreeNodes(nodes))?;
    }
    for issues in tree.issues.chunks(TREE_ISSUE_CHUNK_SIZE) {
        write_frame(writer, &AdminMessageChunk::TreeIssues(issues))?;
    }
    write_frame(writer, &AdminMessage::TreeEnd)
}

#[derive(Debug, Default)]
pub struct ScanTreeAssembler {
    pending: Option<PendingScanTree>,
    finished: bool,
}

#[derive(Debug)]
struct PendingScanTree {
    root_id: u64,
    summary: ScanSummary,
    expected_nodes: usize,
    expected_issues: usize,
    nodes: Vec<ScanNode>,
    issues: Vec<ScanIssue>,
}

impl ScanTreeAssembler {
    pub fn start(
        &mut self,
        root_id: u64,
        summary: ScanSummary,
        node_count: u64,
        issue_count: u64,
    ) -> io::Result<()> {
        if self.pending.is_some() || self.finished {
            return Err(invalid_protocol("duplicate scan tree start"));
        }
        let expected_nodes = usize::try_from(node_count)
            .map_err(|_| invalid_protocol("scan tree node count is unsupported"))?;
        let expected_issues = usize::try_from(issue_count)
            .map_err(|_| invalid_protocol("scan tree issue count is unsupported"))?;
        if expected_nodes == 0 {
            return Err(invalid_protocol("scan tree cannot be empty"));
        }
        if usize::try_from(root_id).map_or(true, |root| root >= expected_nodes) {
            return Err(invalid_protocol("scan tree root is outside the node range"));
        }

        self.pending = Some(PendingScanTree {
            root_id,
            summary,
            expected_nodes,
            expected_issues,
            nodes: Vec::with_capacity(expected_nodes.min(TREE_NODE_CHUNK_SIZE * 4)),
            issues: Vec::with_capacity(expected_issues.min(TREE_ISSUE_CHUNK_SIZE * 4)),
        });
        Ok(())
    }

    pub fn push_nodes(&mut self, nodes: Vec<ScanNode>) -> io::Result<()> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| invalid_protocol("scan tree nodes arrived before tree start"))?;
        let next_len = pending
            .nodes
            .len()
            .checked_add(nodes.len())
            .ok_or_else(|| invalid_protocol("scan tree node count overflow"))?;
        if next_len > pending.expected_nodes {
            return Err(invalid_protocol(
                "scan tree contains more nodes than declared",
            ));
        }

        for (offset, node) in nodes.iter().enumerate() {
            let expected_id = u64::try_from(pending.nodes.len() + offset)
                .map_err(|_| invalid_protocol("scan tree node index overflow"))?;
            if node.id != expected_id {
                return Err(invalid_protocol("scan tree node IDs are not contiguous"));
            }
            if !node.children.is_empty() {
                return Err(invalid_protocol(
                    "scan tree node chunk contains an unexpected child index",
                ));
            }
            if node.parent_id.is_some_and(|parent_id| parent_id >= node.id) {
                return Err(invalid_protocol(
                    "scan tree node refers to an invalid parent",
                ));
            }
        }
        pending.nodes.extend(nodes);
        Ok(())
    }

    pub fn push_issues(&mut self, issues: Vec<ScanIssue>) -> io::Result<()> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| invalid_protocol("scan tree issues arrived before tree start"))?;
        let next_len = pending
            .issues
            .len()
            .checked_add(issues.len())
            .ok_or_else(|| invalid_protocol("scan tree issue count overflow"))?;
        if next_len > pending.expected_issues {
            return Err(invalid_protocol(
                "scan tree contains more issues than declared",
            ));
        }
        pending.issues.extend(issues);
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<ScanTree> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| invalid_protocol("scan tree ended before tree start"))?;
        if pending.nodes.len() != pending.expected_nodes {
            return Err(invalid_protocol("scan tree ended before all nodes arrived"));
        }
        if pending.issues.len() != pending.expected_issues {
            return Err(invalid_protocol(
                "scan tree ended before all issues arrived",
            ));
        }

        let mut pending = self.pending.take().expect("pending tree checked above");
        for index in 0..pending.nodes.len() {
            let id = pending.nodes[index].id;
            let parent_id = pending.nodes[index].parent_id;
            if id == pending.root_id {
                if parent_id.is_some() {
                    return Err(invalid_protocol("scan tree root has a parent"));
                }
            } else {
                let parent_id =
                    parent_id.ok_or_else(|| invalid_protocol("scan tree contains another root"))?;
                let parent = usize::try_from(parent_id)
                    .map_err(|_| invalid_protocol("scan tree parent index is unsupported"))?;
                pending.nodes[parent].children.push(id);
            }
        }
        self.finished = true;
        Ok(ScanTree {
            root_id: pending.root_id,
            nodes: pending.nodes,
            issues: pending.issues,
            summary: pending.summary,
        })
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

fn invalid_protocol(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IssueKind, NodeKind, ScanStatus};
    use std::io::Cursor;
    use std::path::PathBuf;

    fn test_node(id: u64, parent_id: Option<u64>) -> ScanNode {
        let display_path = if id == 0 {
            "/".to_string()
        } else {
            format!("/home/test/file-{id}")
        };
        ScanNode {
            id,
            parent_id,
            name: if id == 0 {
                "/".into()
            } else {
                format!("file-{id}")
            },
            uri: format!("file://{display_path}"),
            local_path: Some(PathBuf::from(&display_path)),
            display_path,
            kind: if id == 0 {
                NodeKind::Directory
            } else {
                NodeKind::File
            },
            allocated_bytes: 4096,
            apparent_bytes: 1024,
            children: if id == 0 {
                (1..600).collect()
            } else {
                Vec::new()
            },
            file_count: if id == 0 { 599 } else { 1 },
            directory_count: u64::from(id == 0),
            modified_ms: None,
            permissions: Some("-rw-r--r--".into()),
            hard_links: 1,
            flags: Vec::new(),
        }
    }

    fn test_tree() -> ScanTree {
        ScanTree {
            root_id: 0,
            nodes: (0..600)
                .map(|id| test_node(id, (id != 0).then_some(0)))
                .collect(),
            issues: vec![ScanIssue {
                path: "/proc".into(),
                kind: IssueKind::Excluded,
                message: "excluded".into(),
            }],
            summary: ScanSummary {
                files: 599,
                directories: 1,
                status: ScanStatus::Complete,
                elevated: true,
                ..ScanSummary::default()
            },
        }
    }

    #[test]
    fn large_tree_round_trips_in_bounded_chunks() {
        let mut wire = Vec::new();
        write_scan_tree(&mut wire, test_tree()).unwrap();

        let mut reader = Cursor::new(wire);
        let mut assembler = ScanTreeAssembler::default();
        let mut node_chunks = 0;
        loop {
            match read_frame::<AdminMessage>(&mut reader).unwrap() {
                AdminMessage::TreeStart {
                    root_id,
                    summary,
                    node_count,
                    issue_count,
                } => assembler
                    .start(root_id, summary, node_count, issue_count)
                    .unwrap(),
                AdminMessage::TreeNodes(nodes) => {
                    node_chunks += 1;
                    assembler.push_nodes(nodes).unwrap();
                }
                AdminMessage::TreeIssues(issues) => assembler.push_issues(issues).unwrap(),
                AdminMessage::TreeEnd => break,
                message => panic!("unexpected message: {message:?}"),
            }
        }
        let tree = assembler.finish().unwrap();
        assert_eq!(node_chunks, 3);
        assert_eq!(tree.nodes.len(), 600);
        assert_eq!(tree.nodes[0].children.len(), 599);
        assert_eq!(tree.nodes[599].parent_id, Some(0));
        assert_eq!(tree.issues.len(), 1);
    }

    #[test]
    fn malformed_tree_chunks_are_rejected() {
        let mut assembler = ScanTreeAssembler::default();
        assert!(assembler.push_nodes(Vec::new()).is_err());

        assembler.start(0, ScanSummary::default(), 2, 0).unwrap();
        assert!(assembler.finish().is_err());
        assert!(assembler.push_nodes(vec![test_node(1, Some(0))]).is_err());
    }

    #[test]
    fn oversized_frame_header_is_rejected_before_allocation() {
        let mut wire = Cursor::new(((MAX_FRAME_SIZE as u64) + 1).to_be_bytes());
        let error = read_frame::<AdminMessage>(&mut wire).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
