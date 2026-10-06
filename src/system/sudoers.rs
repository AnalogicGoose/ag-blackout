use std::collections::HashSet;

use crate::system::UserDatabase;

/// Who may `sudo`. Mirrors a real `/etc/sudoers`: individual users listed by
/// name, or "%group" membership — here root is allowed outright (matching
/// the `root ALL=(ALL:ALL) ALL` line already in `/etc/sudoers`, Phase 1) and
/// anyone in the `sudo` group is allowed (matching `admin`'s membership,
/// Phase 2). Not synced from the actual `/etc/sudoers` file yet — see
/// ARCHITECTURE.md for why System and Filesystem stay decoupled for now.
pub struct Sudoers {
    allowed_users: HashSet<String>,
    allowed_groups: HashSet<String>,
}

impl Sudoers {
    pub fn new() -> Self {
        Sudoers {
            allowed_users: HashSet::from(["root".to_string()]),
            allowed_groups: HashSet::from(["sudo".to_string()]),
        }
    }

    pub fn permits(&self, db: &UserDatabase, uid: u32) -> bool {
        let Some(user) = db.user_by_uid(uid) else {
            return false;
        };
        if self.allowed_users.contains(&user.username) {
            return true;
        }
        db.groups_of(uid)
            .map(|groups| groups.iter().any(|g| self.allowed_groups.contains(&g.name)))
            .unwrap_or(false)
    }
}

impl Default for Sudoers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_always_permitted() {
        let db = UserDatabase::new();
        assert!(Sudoers::new().permits(&db, 0));
    }

    #[test]
    fn sudo_group_member_is_permitted() {
        let db = UserDatabase::new();
        assert!(Sudoers::new().permits(&db, 1000)); // admin, in the sudo group
    }

    #[test]
    fn plain_user_is_not_permitted() {
        let db = UserDatabase::new();
        assert!(!Sudoers::new().permits(&db, 1001)); // guest
    }

    #[test]
    fn unknown_uid_is_not_permitted() {
        let db = UserDatabase::new();
        assert!(!Sudoers::new().permits(&db, 9999));
    }
}
