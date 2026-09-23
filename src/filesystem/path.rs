use std::fmt;

use super::error::FsError;

/// Always represents a normalized, absolute path (root-relative component list).
/// Relative input from the shell is resolved against a cwd via `VirtualPath::resolve`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct VirtualPath {
    components: Vec<String>,
}

impl VirtualPath {
    pub fn root() -> Self {
        VirtualPath { components: Vec::new() }
    }

    pub fn from_components(components: Vec<String>) -> Self {
        VirtualPath { components }
    }

    /// Resolves `input` (absolute or relative) against `cwd`, collapsing `.` and `..`.
    /// `..` past root is clamped to root, matching shell behavior.
    pub fn resolve(cwd: &VirtualPath, input: &str) -> Result<VirtualPath, FsError> {
        if input.is_empty() {
            return Err(FsError::InvalidPath(input.to_string()));
        }
        let mut components = if input.starts_with('/') {
            Vec::new()
        } else {
            cwd.components.clone()
        };
        for part in input.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                seg => {
                    if seg.contains('\0') {
                        return Err(FsError::InvalidPath(input.to_string()));
                    }
                    components.push(seg.to_string());
                }
            }
        }
        Ok(VirtualPath { components })
    }

    pub fn is_root(&self) -> bool {
        self.components.is_empty()
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(String::as_str)
    }

    pub fn parent(&self) -> Option<VirtualPath> {
        if self.components.is_empty() {
            None
        } else {
            let mut c = self.components.clone();
            c.pop();
            Some(VirtualPath { components: c })
        }
    }

    pub fn join(&self, name: &str) -> VirtualPath {
        let mut c = self.components.clone();
        c.push(name.to_string());
        VirtualPath { components: c }
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.components.is_empty() {
            write!(f, "/")
        } else {
            write!(f, "/{}", self.components.join("/"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_absolute_paths() {
        let cwd = VirtualPath::root();
        let p = VirtualPath::resolve(&cwd, "/etc/passwd").unwrap();
        assert_eq!(p.to_string(), "/etc/passwd");
    }

    #[test]
    fn resolves_relative_paths_against_cwd() {
        let cwd = VirtualPath::resolve(&VirtualPath::root(), "/home/guest").unwrap();
        let p = VirtualPath::resolve(&cwd, "docs").unwrap();
        assert_eq!(p.to_string(), "/home/guest/docs");
    }

    #[test]
    fn collapses_dot_and_dotdot() {
        let cwd = VirtualPath::resolve(&VirtualPath::root(), "/home/guest").unwrap();
        let p = VirtualPath::resolve(&cwd, "../../etc/./passwd").unwrap();
        assert_eq!(p.to_string(), "/etc/passwd");
    }

    #[test]
    fn dotdot_past_root_clamps_to_root() {
        let p = VirtualPath::resolve(&VirtualPath::root(), "/../../..").unwrap();
        assert_eq!(p.to_string(), "/");
    }

    #[test]
    fn collapses_duplicate_slashes_and_trailing_slash() {
        let p = VirtualPath::resolve(&VirtualPath::root(), "/etc//ssh/").unwrap();
        assert_eq!(p.to_string(), "/etc/ssh");
    }

    #[test]
    fn empty_input_is_invalid() {
        assert!(VirtualPath::resolve(&VirtualPath::root(), "").is_err());
    }
}