use crate::filesystem::VirtualPath;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractStatus {
    Active,
    Completed,
}

/// Slice 1: exactly one hardcoded objective kind — obtain (read, while
/// authenticated) a specific resource on a specific device. Shaped so a
/// future `Objective` enum (ReadResource/CopyResource/ModifyResource/...)
/// can replace the implicit "read" meaning without touching this struct's
/// other fields or how contracts are stored/resolved. See docs/GAME_DESIGN.md.
#[derive(Clone, Debug)]
pub struct Contract {
    pub title: String,
    pub target_hostname: String,
    pub resource_path: VirtualPath,
    pub reward: i64,
    pub status: ContractStatus,
}

impl Contract {
    pub fn new(title: impl Into<String>, target_hostname: impl Into<String>, resource_path: VirtualPath, reward: i64) -> Self {
        Contract {
            title: title.into(),
            target_hostname: target_hostname.into(),
            resource_path,
            reward,
            status: ContractStatus::Active,
        }
    }
}
