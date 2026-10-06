use std::collections::VecDeque;
use std::time::SystemTime;

use super::error::FsError;
use super::metadata::Metadata;
use super::node::{Node, NodeKind};
use super::path::VirtualPath;
use super::permissions::{AccessMode, FsAccess, Mode};

const MAX_SYMLINK_HOPS: u32 = 40;

const ROOT_UID: u32 = 0;
const ROOT_GID: u32 = 0;
const GUEST_UID: u32 = 1001;
const GUEST_GID: u32 = 1001;

const OS_RELEASE: &str = "NAME=\"AG Linux\"\n\
PRETTY_NAME=\"AG Linux 1.0.0 (Blackbird)\"\n\
ID=\"aglinux\"\n\
VERSION_ID=\"1.0\"\n\
VERSION_CODENAME=\"blackbird\"\n";

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub node_type: char,
    pub mode: Mode,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub size: u64,
    pub modified_at: SystemTime,
}

impl DirEntry {
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    /// e.g. "-rwxr-xr-x" as `ls -l` would show it.
    pub fn permissions_string(&self) -> String {
        format!("{}{}", self.node_type, self.mode)
    }
}

pub struct VirtualFS {
    root: Node,
}

impl VirtualFS {
    pub fn new() -> Self {
        let root = Node::new_dir(Metadata::new(Mode::rwx_dir(), ROOT_UID, ROOT_GID));
        let mut fs = VirtualFS { root };
        fs.build_default_tree();
        fs
    }

    // ---- path resolution -------------------------------------------------

    fn check_access(access: &FsAccess, node: &Node, mode: AccessMode) -> Result<(), FsError> {
        if access.is_superuser {
            return Ok(());
        }
        let class = access.class_for(node.metadata.owner_uid, node.metadata.group_gid);
        if node.metadata.mode.allows(class, mode) {
            Ok(())
        } else {
            Err(FsError::PermissionDenied(mode_label(mode).to_string()))
        }
    }

    fn get(&self, path: &VirtualPath) -> Result<&Node, FsError> {
        let mut current = &self.root;
        for name in path.components() {
            let children = current
                .children()
                .ok_or_else(|| FsError::NotADirectory(path.to_string()))?;
            current = children
                .get(name)
                .ok_or_else(|| FsError::NotFound(path.to_string()))?;
        }
        Ok(current)
    }

    fn get_mut(&mut self, path: &VirtualPath) -> Result<&mut Node, FsError> {
        let mut current = &mut self.root;
        for name in path.components() {
            let children = current
                .children_mut()
                .ok_or_else(|| FsError::NotADirectory(path.to_string()))?;
            current = children
                .get_mut(name)
                .ok_or_else(|| FsError::NotFound(path.to_string()))?;
        }
        Ok(current)
    }

    fn split_parent(&self, path: &VirtualPath) -> Result<(VirtualPath, String), FsError> {
        let name = path
            .file_name()
            .ok_or_else(|| FsError::InvalidPath(path.to_string()))?
            .to_string();
        let parent = path
            .parent()
            .ok_or_else(|| FsError::InvalidPath(path.to_string()))?;
        Ok((parent, name))
    }

    /// Walks `path` from the root, checking execute (search) permission on
    /// every ancestor directory and following symlinks encountered along the
    /// way (including in the final component, unless `follow_final_symlink`
    /// is false — used by `readlink`/`lstat`-style operations).
    fn resolve_path(
        &self,
        access: &FsAccess,
        path: &VirtualPath,
        follow_final_symlink: bool,
    ) -> Result<VirtualPath, FsError> {
        let mut resolved: Vec<String> = Vec::new();
        let mut queue: VecDeque<String> = path.components().iter().cloned().collect();
        let mut hops = 0u32;

        while let Some(name) = queue.pop_front() {
            let current_path = VirtualPath::from_components(resolved.clone());
            let current = self.get(&current_path)?;
            if !current.is_dir() {
                return Err(FsError::NotADirectory(current_path.to_string()));
            }
            Self::check_access(access, current, AccessMode::Execute)?;

            let children = current.children().unwrap();
            let child = children
                .get(&name)
                .ok_or_else(|| FsError::NotFound(current_path.join(&name).to_string()))?;

            if let NodeKind::Symlink { target } = &child.kind {
                let is_last = queue.is_empty();
                if is_last && !follow_final_symlink {
                    resolved.push(name);
                    break;
                }
                hops += 1;
                if hops > MAX_SYMLINK_HOPS {
                    return Err(FsError::SymlinkLoop(path.to_string()));
                }
                let base = VirtualPath::from_components(resolved.clone());
                let target_path = if target.starts_with('/') {
                    VirtualPath::resolve(&VirtualPath::root(), target)?
                } else {
                    VirtualPath::resolve(&base, target)?
                };
                let mut new_queue: VecDeque<String> =
                    target_path.components().iter().cloned().collect();
                new_queue.extend(queue);
                queue = new_queue;
                resolved.clear();
                continue;
            }

            resolved.push(name);
        }

        Ok(VirtualPath::from_components(resolved))
    }

    // ---- metadata / reads --------------------------------------------------

    pub fn metadata(&self, access: &FsAccess, path: &VirtualPath) -> Result<Metadata, FsError> {
        let resolved = self.resolve_path(access, path, true)?;
        Ok(self.get(&resolved)?.metadata.clone())
    }

    pub fn symlink_metadata(
        &self,
        access: &FsAccess,
        path: &VirtualPath,
    ) -> Result<Metadata, FsError> {
        let resolved = self.resolve_path(access, path, false)?;
        Ok(self.get(&resolved)?.metadata.clone())
    }

    /// Whether `path` is a directory the caller may `cd` into — needs execute
    /// (search) on the target itself, deliberately not read, matching real
    /// `chdir(2)`: you can `cd` into a directory you can't `ls`.
    pub fn is_dir(&self, access: &FsAccess, path: &VirtualPath) -> Result<bool, FsError> {
        let resolved = self.resolve_path(access, path, true)?;
        let node = self.get(&resolved)?;
        if node.is_dir() {
            Self::check_access(access, node, AccessMode::Execute)?;
        }
        Ok(node.is_dir())
    }

    pub fn read_file(&self, access: &FsAccess, path: &VirtualPath) -> Result<Vec<u8>, FsError> {
        let resolved = self.resolve_path(access, path, true)?;
        let node = self.get(&resolved)?;
        match &node.kind {
            NodeKind::File { content } => {
                Self::check_access(access, node, AccessMode::Read)?;
                Ok(content.clone())
            }
            NodeKind::Directory { .. } => Err(FsError::IsADirectory(resolved.to_string())),
            NodeKind::Symlink { .. } => {
                unreachable!("resolve_path always follows the final symlink here")
            }
        }
    }

    pub fn list_dir(
        &self,
        access: &FsAccess,
        path: &VirtualPath,
    ) -> Result<Vec<DirEntry>, FsError> {
        let resolved = self.resolve_path(access, path, true)?;
        let node = self.get(&resolved)?;
        if !node.is_dir() {
            return Err(FsError::NotADirectory(resolved.to_string()));
        }
        Self::check_access(access, node, AccessMode::Read)?;
        let children = node.children().unwrap();
        Ok(children
            .iter()
            .map(|(name, n)| DirEntry {
                name: name.clone(),
                node_type: n.type_char(),
                mode: n.metadata.mode,
                owner_uid: n.metadata.owner_uid,
                group_gid: n.metadata.group_gid,
                size: n.len(),
                modified_at: n.metadata.modified_at,
            })
            .collect())
    }

    pub fn read_link(&self, access: &FsAccess, path: &VirtualPath) -> Result<String, FsError> {
        let resolved = self.resolve_path(access, path, false)?;
        let node = self.get(&resolved)?;
        match &node.kind {
            NodeKind::Symlink { target } => Ok(target.clone()),
            _ => Err(FsError::NotASymlink(path.to_string())),
        }
    }

    // ---- writes -------------------------------------------------------------

    fn create_node(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        node: Node,
    ) -> Result<(), FsError> {
        let (parent_path, name) = self.split_parent(path)?;
        let resolved_parent = self.resolve_path(access, &parent_path, true)?;
        let parent = self.get_mut(&resolved_parent)?;
        if !parent.is_dir() {
            return Err(FsError::NotADirectory(resolved_parent.to_string()));
        }
        Self::check_access(access, parent, AccessMode::Write)?;
        Self::check_access(access, parent, AccessMode::Execute)?;
        let children = parent.children_mut().unwrap();
        if children.contains_key(&name) {
            return Err(FsError::AlreadyExists(path.to_string()));
        }
        children.insert(name, node);
        parent.metadata.touch();
        Ok(())
    }

    pub fn mkdir(&mut self, access: &FsAccess, path: &VirtualPath) -> Result<(), FsError> {
        let metadata = Metadata::new(Mode::rwx_dir(), access.uid, access.primary_gid());
        self.create_node(access, path, Node::new_dir(metadata))
    }

    fn create_file(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        contents: &[u8],
    ) -> Result<(), FsError> {
        let metadata = Metadata::new(Mode::rw_file(), access.uid, access.primary_gid());
        self.create_node(access, path, Node::new_file(metadata, contents.to_vec()))
    }

    pub fn touch(&mut self, access: &FsAccess, path: &VirtualPath) -> Result<(), FsError> {
        match self.resolve_path(access, path, true) {
            Ok(resolved) => {
                let node = self.get_mut(&resolved)?;
                node.metadata.touch();
                Ok(())
            }
            Err(FsError::NotFound(_)) => self.create_file(access, path, &[]),
            Err(e) => Err(e),
        }
    }

    pub fn write_file(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        contents: &[u8],
    ) -> Result<(), FsError> {
        match self.resolve_path(access, path, true) {
            Ok(resolved) => {
                let node = self.get_mut(&resolved)?;
                if node.is_dir() {
                    return Err(FsError::IsADirectory(resolved.to_string()));
                }
                Self::check_access(access, node, AccessMode::Write)?;
                match &mut node.kind {
                    NodeKind::File { content } => *content = contents.to_vec(),
                    _ => unreachable!("dir case handled above; final symlink was followed"),
                }
                node.metadata.touch();
                Ok(())
            }
            Err(FsError::NotFound(_)) => self.create_file(access, path, contents),
            Err(e) => Err(e),
        }
    }

    pub fn symlink(
        &mut self,
        access: &FsAccess,
        target: &str,
        link_path: &VirtualPath,
    ) -> Result<(), FsError> {
        let metadata = Metadata::new(Mode::new(0o777), access.uid, access.primary_gid());
        self.create_node(
            access,
            link_path,
            Node::new_symlink(metadata, target.to_string()),
        )
    }

    pub fn remove_file(&mut self, access: &FsAccess, path: &VirtualPath) -> Result<(), FsError> {
        let (parent_path, name) = self.split_parent(path)?;
        let resolved_parent = self.resolve_path(access, &parent_path, true)?;
        let parent = self.get_mut(&resolved_parent)?;
        Self::check_access(access, parent, AccessMode::Write)?;
        Self::check_access(access, parent, AccessMode::Execute)?;
        let children = parent.children_mut().unwrap();
        let node = children
            .get(&name)
            .ok_or_else(|| FsError::NotFound(path.to_string()))?;
        if node.is_dir() {
            return Err(FsError::IsADirectory(path.to_string()));
        }
        children.remove(&name);
        parent.metadata.touch();
        Ok(())
    }

    pub fn remove_dir(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        recursive: bool,
    ) -> Result<(), FsError> {
        let (parent_path, name) = self.split_parent(path)?;
        let resolved_parent = self.resolve_path(access, &parent_path, true)?;
        let parent = self.get_mut(&resolved_parent)?;
        Self::check_access(access, parent, AccessMode::Write)?;
        Self::check_access(access, parent, AccessMode::Execute)?;
        let children = parent.children_mut().unwrap();
        let node = children
            .get(&name)
            .ok_or_else(|| FsError::NotFound(path.to_string()))?;
        if !node.is_dir() {
            return Err(FsError::NotADirectory(path.to_string()));
        }
        if !recursive && !node.is_empty() {
            return Err(FsError::NotEmpty(path.to_string()));
        }
        children.remove(&name);
        parent.metadata.touch();
        Ok(())
    }

    pub fn rename(
        &mut self,
        access: &FsAccess,
        from: &VirtualPath,
        to: &VirtualPath,
    ) -> Result<(), FsError> {
        let (from_parent_path, from_name) = self.split_parent(from)?;
        let resolved_from_parent = self.resolve_path(access, &from_parent_path, true)?;

        let (to_parent_path, to_name) = self.split_parent(to)?;
        let resolved_to_parent = self.resolve_path(access, &to_parent_path, true)?;

        {
            let from_parent = self.get(&resolved_from_parent)?;
            Self::check_access(access, from_parent, AccessMode::Write)?;
            Self::check_access(access, from_parent, AccessMode::Execute)?;
            if !from_parent
                .children()
                .map(|c| c.contains_key(&from_name))
                .unwrap_or(false)
            {
                return Err(FsError::NotFound(from.to_string()));
            }

            let to_parent = self.get(&resolved_to_parent)?;
            Self::check_access(access, to_parent, AccessMode::Write)?;
            Self::check_access(access, to_parent, AccessMode::Execute)?;
            if to_parent
                .children()
                .map(|c| c.contains_key(&to_name))
                .unwrap_or(false)
            {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
        }

        let node = self
            .get_mut(&resolved_from_parent)?
            .children_mut()
            .unwrap()
            .remove(&from_name)
            .ok_or_else(|| FsError::NotFound(from.to_string()))?;
        self.get_mut(&resolved_from_parent)?.metadata.touch();

        self.get_mut(&resolved_to_parent)?
            .children_mut()
            .unwrap()
            .insert(to_name, node);
        self.get_mut(&resolved_to_parent)?.metadata.touch();

        Ok(())
    }

    pub fn copy_file(
        &mut self,
        access: &FsAccess,
        from: &VirtualPath,
        to: &VirtualPath,
    ) -> Result<(), FsError> {
        let contents = self.read_file(access, from)?;
        self.create_file(access, to, &contents)
    }

    pub fn chmod(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        mode: Mode,
    ) -> Result<(), FsError> {
        let resolved = self.resolve_path(access, path, true)?;
        let node = self.get_mut(&resolved)?;
        if !access.is_superuser && access.uid != node.metadata.owner_uid {
            return Err(FsError::PermissionDenied(path.to_string()));
        }
        node.metadata.mode = mode;
        node.metadata.touch();
        Ok(())
    }

    pub fn chown(
        &mut self,
        access: &FsAccess,
        path: &VirtualPath,
        owner_uid: u32,
        group_gid: u32,
    ) -> Result<(), FsError> {
        if !access.is_superuser {
            return Err(FsError::PermissionDenied(path.to_string()));
        }
        let resolved = self.resolve_path(access, path, true)?;
        let node = self.get_mut(&resolved)?;
        node.metadata.owner_uid = owner_uid;
        node.metadata.group_gid = group_gid;
        node.metadata.touch();
        Ok(())
    }
    // ---- default AG Linux tree ----------------------------------------------

    fn build_default_tree(&mut self) {
        let root_access = FsAccess::root();
        let p = |s: &str| VirtualPath::resolve(&VirtualPath::root(), s).unwrap();

        for dir in [
            "bin", "boot", "dev", "etc", "home", "lib", "opt", "proc", "root", "run", "tmp", "usr",
            "var",
        ] {
            self.mkdir(&root_access, &p(dir)).unwrap();
        }
        for dir in [
            "etc/ssh",
            "etc/agpkg",
            "var/cache",
            "var/lib",
            "var/log",
            "var/www",
            "home/guest",
        ] {
            self.mkdir(&root_access, &p(dir)).unwrap();
        }
        for dir in ["var/cache/agpkg", "var/lib/agpkg"] {
            self.mkdir(&root_access, &p(dir)).unwrap();
        }

        self.write_file(&root_access, &p("etc/hostname"), b"web01\n")
            .unwrap();
        self.write_file(&root_access, &p("etc/hosts"), b"127.0.0.1\tlocalhost\n")
            .unwrap();
        self.write_file(
            &root_access,
            &p("etc/passwd"),
            b"root:x:0:0:root:/root:/bin/bash\nguest:x:1001:1001:guest:/home/guest:/bin/bash\n",
        )
        .unwrap();
        self.write_file(
            &root_access,
            &p("etc/shadow"),
            b"root:!:19700:0:99999:7:::\nguest:!:19700:0:99999:7:::\n",
        )
        .unwrap();
        self.chmod(&root_access, &p("etc/shadow"), Mode::new(0o600))
            .unwrap();

        self.write_file(&root_access, &p("etc/sudoers"), b"root ALL=(ALL:ALL) ALL\n")
            .unwrap();
        self.chmod(&root_access, &p("etc/sudoers"), Mode::new(0o440))
            .unwrap();

        self.write_file(
            &root_access,
            &p("etc/agpkg/repos.conf"),
            b"# AGPKG repositories\n",
        )
        .unwrap();
        self.write_file(&root_access, &p("etc/os-release"), OS_RELEASE.as_bytes())
            .unwrap();

        self.chown(&root_access, &p("home/guest"), GUEST_UID, GUEST_GID)
            .unwrap();
        self.chmod(&root_access, &p("home/guest"), Mode::new(0o700))
            .unwrap();

        self.chown(&root_access, &p("root"), ROOT_UID, ROOT_GID)
            .unwrap();
        self.chmod(&root_access, &p("root"), Mode::new(0o700))
            .unwrap();

        // World-writable so any user can drop scratch files there, matching
        // real /tmp. We don't model the sticky bit yet (Mode is 9 bits, no
        // "only the owner may delete their own file here" protection) — fine
        // for now, worth revisiting if that distinction ever matters for gameplay.
        self.chmod(&root_access, &p("tmp"), Mode::new(0o777))
            .unwrap();
    }
}

impl Default for VirtualFS {
    fn default() -> Self {
        Self::new()
    }
}

fn mode_label(mode: AccessMode) -> &'static str {
    match mode {
        AccessMode::Read => "read",
        AccessMode::Write => "write",
        AccessMode::Execute => "execute",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> FsAccess {
        FsAccess::root()
    }

    fn guest() -> FsAccess {
        FsAccess::new(GUEST_UID, vec![GUEST_GID])
    }

    fn path(s: &str) -> VirtualPath {
        VirtualPath::resolve(&VirtualPath::root(), s).unwrap()
    }

    #[test]
    fn default_tree_has_expected_layout() {
        let fs = VirtualFS::new();
        let entries = fs.list_dir(&root(), &path("/etc")).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"passwd"));
        assert!(names.contains(&"shadow"));
        assert!(names.contains(&"sudoers"));
        assert!(names.contains(&"agpkg"));
    }

    #[test]
    fn mkdir_and_ls() {
        let mut fs = VirtualFS::new();
        fs.mkdir(&root(), &path("/tmp/project")).unwrap();
        let entries = fs.list_dir(&root(), &path("/tmp")).unwrap();
        assert!(
            entries
                .iter()
                .any(|e| e.name == "project" && e.node_type == 'd')
        );
    }

    #[test]
    fn mkdir_existing_fails() {
        let mut fs = VirtualFS::new();
        fs.mkdir(&root(), &path("/tmp/project")).unwrap();
        let err = fs.mkdir(&root(), &path("/tmp/project")).unwrap_err();
        assert_eq!(err, FsError::AlreadyExists("/tmp/project".to_string()));
    }

    #[test]
    fn write_then_read_file() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/tmp/note.txt"), b"hello")
            .unwrap();
        assert_eq!(
            fs.read_file(&root(), &path("/tmp/note.txt")).unwrap(),
            b"hello"
        );
    }

    #[test]
    fn touch_creates_empty_file_and_updates_mtime() {
        let mut fs = VirtualFS::new();
        fs.touch(&root(), &path("/tmp/empty")).unwrap();
        assert_eq!(fs.read_file(&root(), &path("/tmp/empty")).unwrap(), b"");
        let before = fs
            .metadata(&root(), &path("/tmp/empty"))
            .unwrap()
            .modified_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs.touch(&root(), &path("/tmp/empty")).unwrap();
        let after = fs
            .metadata(&root(), &path("/tmp/empty"))
            .unwrap()
            .modified_at;
        assert!(after > before);
    }

    #[test]
    fn ownership_is_set_from_access() {
        let mut fs = VirtualFS::new();
        fs.write_file(&guest(), &path("/home/guest/note.txt"), b"hi")
            .unwrap();
        let meta = fs.metadata(&root(), &path("/home/guest/note.txt")).unwrap();
        assert_eq!(meta.owner_uid, GUEST_UID);
        assert_eq!(meta.group_gid, GUEST_GID);
    }

    #[test]
    fn hidden_files_are_detected_by_leading_dot() {
        let mut fs = VirtualFS::new();
        fs.touch(&root(), &path("/tmp/.secret")).unwrap();
        fs.touch(&root(), &path("/tmp/visible")).unwrap();
        let entries = fs.list_dir(&root(), &path("/tmp")).unwrap();
        let secret = entries.iter().find(|e| e.name == ".secret").unwrap();
        let visible = entries.iter().find(|e| e.name == "visible").unwrap();
        assert!(secret.is_hidden());
        assert!(!visible.is_hidden());
    }

    #[test]
    fn guest_cannot_read_shadow() {
        let fs = VirtualFS::new();
        let err = fs.read_file(&guest(), &path("/etc/shadow")).unwrap_err();
        assert!(matches!(err, FsError::PermissionDenied(_)));
    }

    #[test]
    fn root_can_read_shadow() {
        let fs = VirtualFS::new();
        assert!(fs.read_file(&root(), &path("/etc/shadow")).is_ok());
    }

    #[test]
    fn guest_cannot_write_outside_home() {
        let mut fs = VirtualFS::new();
        let err = fs
            .write_file(&guest(), &path("/etc/hosts"), b"evil")
            .unwrap_err();
        assert!(matches!(err, FsError::PermissionDenied(_)));
    }

    #[test]
    fn guest_cannot_enter_root_home() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/root/secret.txt"), b"top secret")
            .unwrap();
        let err = fs
            .read_file(&guest(), &path("/root/secret.txt"))
            .unwrap_err();
        assert!(matches!(err, FsError::PermissionDenied(_)));
    }

    #[test]
    fn chmod_only_owner_or_root() {
        let mut fs = VirtualFS::new();
        fs.write_file(&guest(), &path("/home/guest/note.txt"), b"hi")
            .unwrap();
        let intruder = FsAccess::new(9999, vec![9999]);
        let err = fs
            .chmod(&intruder, &path("/home/guest/note.txt"), Mode::new(0o777))
            .unwrap_err();
        assert!(matches!(err, FsError::PermissionDenied(_)));
        fs.chmod(&guest(), &path("/home/guest/note.txt"), Mode::new(0o600))
            .unwrap();
    }

    #[test]
    fn symlink_resolves_to_target() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/etc/real.conf"), b"data")
            .unwrap();
        fs.symlink(&root(), "/etc/real.conf", &path("/etc/alias.conf"))
            .unwrap();
        assert_eq!(
            fs.read_file(&root(), &path("/etc/alias.conf")).unwrap(),
            b"data"
        );
    }

    #[test]
    fn readlink_returns_target_without_following() {
        let mut fs = VirtualFS::new();
        fs.symlink(&root(), "/etc/hostname", &path("/etc/alias"))
            .unwrap();
        assert_eq!(
            fs.read_link(&root(), &path("/etc/alias")).unwrap(),
            "/etc/hostname"
        );
    }

    #[test]
    fn symlink_loop_is_detected() {
        let mut fs = VirtualFS::new();
        fs.symlink(&root(), "/tmp/b", &path("/tmp/a")).unwrap();
        fs.symlink(&root(), "/tmp/a", &path("/tmp/b")).unwrap();
        let err = fs.read_file(&root(), &path("/tmp/a")).unwrap_err();
        assert!(matches!(err, FsError::SymlinkLoop(_)));
    }

    #[test]
    fn copy_file_duplicates_contents() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/tmp/src.txt"), b"payload")
            .unwrap();
        fs.copy_file(&root(), &path("/tmp/src.txt"), &path("/tmp/dst.txt"))
            .unwrap();
        assert_eq!(
            fs.read_file(&root(), &path("/tmp/dst.txt")).unwrap(),
            b"payload"
        );
        assert_eq!(
            fs.read_file(&root(), &path("/tmp/src.txt")).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn rename_moves_node_between_directories() {
        let mut fs = VirtualFS::new();
        fs.mkdir(&root(), &path("/tmp/dest")).unwrap();
        fs.write_file(&root(), &path("/tmp/src.txt"), b"payload")
            .unwrap();
        fs.rename(&root(), &path("/tmp/src.txt"), &path("/tmp/dest/moved.txt"))
            .unwrap();
        assert!(fs.read_file(&root(), &path("/tmp/src.txt")).is_err());
        assert_eq!(
            fs.read_file(&root(), &path("/tmp/dest/moved.txt")).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn remove_file_deletes_entry() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/tmp/gone.txt"), b"x")
            .unwrap();
        fs.remove_file(&root(), &path("/tmp/gone.txt")).unwrap();
        assert!(matches!(
            fs.read_file(&root(), &path("/tmp/gone.txt")).unwrap_err(),
            FsError::NotFound(_)
        ));
    }

    #[test]
    fn remove_dir_requires_empty_unless_recursive() {
        let mut fs = VirtualFS::new();
        fs.mkdir(&root(), &path("/tmp/dir")).unwrap();
        fs.write_file(&root(), &path("/tmp/dir/file.txt"), b"x")
            .unwrap();

        let err = fs
            .remove_dir(&root(), &path("/tmp/dir"), false)
            .unwrap_err();
        assert!(matches!(err, FsError::NotEmpty(_)));

        fs.remove_dir(&root(), &path("/tmp/dir"), true).unwrap();
        assert!(matches!(
            fs.list_dir(&root(), &path("/tmp/dir")).unwrap_err(),
            FsError::NotFound(_)
        ));
    }

    #[test]
    fn cannot_read_file_as_directory() {
        let mut fs = VirtualFS::new();
        fs.write_file(&root(), &path("/tmp/file.txt"), b"x")
            .unwrap();
        let err = fs.list_dir(&root(), &path("/tmp/file.txt")).unwrap_err();
        assert!(matches!(err, FsError::NotADirectory(_)));
    }
}
