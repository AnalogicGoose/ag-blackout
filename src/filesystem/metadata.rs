use std::time::SystemTime;

use super::permissions::Mode;

#[derive(Clone, Debug)]
pub struct Metadata {
    pub mode: Mode,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
}

impl Metadata {
    pub fn new(mode: Mode, owner_uid: u32, group_gid: u32) -> Self {
        let now = SystemTime::now();
        Metadata { mode, owner_uid, group_gid, created_at: now, modified_at: now }
    }

    pub fn touch(&mut self) {
        self.modified_at = SystemTime::now();
    }
}