use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FsError {
    #[error("no such file or directory: {0}")]
    NotFound(String),
    #[error("file exists: {0}")]
    AlreadyExists(String),
    #[error("not a directory: {0}")]
    NotADirectory(String),
    #[error("is a directory: {0}")]
    IsADirectory(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("directory not empty: {0}")]
    NotEmpty(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("too many levels of symbolic links: {0}")]
    SymlinkLoop(String),
    #[error("not a symbolic link: {0}")]
    NotASymlink(String),
}
