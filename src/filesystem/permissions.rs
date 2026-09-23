use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessClass {
    Owner,
    Group,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    Write,
    Execute,
}

/// Classic rwxrwxrwx bit field (owner/group/other), stored as the low 9 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode(u16);

impl Mode {
    pub const OWNER_READ: u16 = 0o400;
    pub const OWNER_WRITE: u16 = 0o200;
    pub const OWNER_EXEC: u16 = 0o100;
    pub const GROUP_READ: u16 = 0o040;
    pub const GROUP_WRITE: u16 = 0o020;
    pub const GROUP_EXEC: u16 = 0o010;
    pub const OTHER_READ: u16 = 0o004;
    pub const OTHER_WRITE: u16 = 0o002;
    pub const OTHER_EXEC: u16 = 0o001;

    pub fn new(bits: u16) -> Self {
        Mode(bits & 0o777)
    }

    pub fn bits(&self) -> u16 {
        self.0
    }

    pub fn rwx_dir() -> Self {
        Mode(0o755)
    }

    pub fn rw_file() -> Self {
        Mode(0o644)
    }

    fn class_bits(class: AccessClass) -> (u16, u16, u16) {
        match class {
            AccessClass::Owner => (Self::OWNER_READ, Self::OWNER_WRITE, Self::OWNER_EXEC),
            AccessClass::Group => (Self::GROUP_READ, Self::GROUP_WRITE, Self::GROUP_EXEC),
            AccessClass::Other => (Self::OTHER_READ, Self::OTHER_WRITE, Self::OTHER_EXEC),
        }
    }

    pub fn allows(&self, class: AccessClass, perm: AccessMode) -> bool {
        let (r, w, x) = Self::class_bits(class);
        let bit = match perm {
            AccessMode::Read => r,
            AccessMode::Write => w,
            AccessMode::Execute => x,
        };
        self.0 & bit != 0
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::with_capacity(9);
        for class in [AccessClass::Owner, AccessClass::Group, AccessClass::Other] {
            s.push(if self.allows(class, AccessMode::Read) { 'r' } else { '-' });
            s.push(if self.allows(class, AccessMode::Write) { 'w' } else { '-' });
            s.push(if self.allows(class, AccessMode::Execute) { 'x' } else { '-' });
        }
        write!(f, "{s}")
    }
}

/// What VirtualFS needs to evaluate a permission check. Phase 2/3's
/// `system::ExecutionContext` will build one of these from the logged-in user
/// on every command; filesystem stays decoupled from the user/session model
/// so Phase 1 can be built and tested without it.
#[derive(Clone, Debug)]
pub struct FsAccess {
    pub uid: u32,
    pub gids: Vec<u32>,
    pub is_superuser: bool,
}

impl FsAccess {
    pub fn root() -> Self {
        FsAccess { uid: 0, gids: vec![0], is_superuser: true }
    }

    pub fn new(uid: u32, gids: Vec<u32>) -> Self {
        let is_superuser = uid == 0;
        FsAccess { uid, gids, is_superuser }
    }

    pub fn primary_gid(&self) -> u32 {
        self.gids.first().copied().unwrap_or(self.uid)
    }

    pub fn class_for(&self, owner_uid: u32, group_gid: u32) -> AccessClass {
        if self.uid == owner_uid {
            AccessClass::Owner
        } else if self.gids.contains(&group_gid) {
            AccessClass::Group
        } else {
            AccessClass::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_display_matches_ls_format() {
        assert_eq!(Mode::new(0o755).to_string(), "rwxr-xr-x");
        assert_eq!(Mode::new(0o644).to_string(), "rw-r--r--");
        assert_eq!(Mode::new(0o600).to_string(), "rw-------");
    }

    #[test]
    fn access_class_resolution() {
        let access = FsAccess::new(1001, vec![1001]);
        assert_eq!(access.class_for(1001, 1001), AccessClass::Owner);
        assert_eq!(access.class_for(0, 1001), AccessClass::Group);
        assert_eq!(access.class_for(0, 0), AccessClass::Other);
    }
}