use std::collections::BTreeMap;

use super::metadata::Metadata;

#[derive(Clone, Debug)]
pub enum NodeKind {
    File { content: Vec<u8> },
    Directory { children: BTreeMap<String, Node> },
    Symlink { target: String },
}

#[derive(Clone, Debug)]
pub struct Node {
    pub metadata: Metadata,
    pub kind: NodeKind,
}

impl Node {
    pub fn new_file(metadata: Metadata, content: Vec<u8>) -> Self {
        Node { metadata, kind: NodeKind::File { content } }
    }

    pub fn new_dir(metadata: Metadata) -> Self {
        Node { metadata, kind: NodeKind::Directory { children: BTreeMap::new() } }
    }

    pub fn new_symlink(metadata: Metadata, target: String) -> Self {
        Node { metadata, kind: NodeKind::Symlink { target } }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.kind, NodeKind::Directory { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, NodeKind::File { .. })
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self.kind, NodeKind::Symlink { .. })
    }

    pub fn type_char(&self) -> char {
        match self.kind {
            NodeKind::File { .. } => '-',
            NodeKind::Directory { .. } => 'd',
            NodeKind::Symlink { .. } => 'l',
        }
    }

    /// Byte length for files/symlinks, entry count for directories — matches
    /// what `ls -l` would show without keeping a separate, staleness-prone field.
    pub fn len(&self) -> u64 {
        match &self.kind {
            NodeKind::File { content } => content.len() as u64,
            NodeKind::Symlink { target } => target.len() as u64,
            NodeKind::Directory { children } => children.len() as u64,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn children(&self) -> Option<&BTreeMap<String, Node>> {
        match &self.kind {
            NodeKind::Directory { children } => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut BTreeMap<String, Node>> {
        match &mut self.kind {
            NodeKind::Directory { children } => Some(children),
            _ => None,
        }
    }
}