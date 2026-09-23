use std::collections::BTreeMap;
use std::fmt;

use crate::filesystem::VirtualPath;

use super::group::Group;
use super::user::{PasswordState, User};

const ROOT_UID: u32 = 0;
const ROOT_GID: u32 = 0;
const SUDO_GID: u32 = 27;
const ADMIN_UID: u32 = 1000;
const ADMIN_GID: u32 = 1000;
const GUEST_UID: u32 = 1001;
const GUEST_GID: u32 = 1001;
const WWW_DATA_UID: u32 = 33;
const WWW_DATA_GID: u32 = 33;

#[derive(Clone, Debug)]
pub struct IdInfo {
    pub uid: u32,
    pub username: String,
    pub gid: u32,
    pub group_name: String,
    /// Primary group first, then supplementary groups — matches `id`'s order.
    pub groups: Vec<(u32, String)>,
}

impl fmt::Display for IdInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let groups = self
            .groups
            .iter()
            .map(|(gid, name)| format!("{gid}({name})"))
            .collect::<Vec<_>>()
            .join(",");
        write!(
            f,
            "uid={}({}) gid={}({}) groups={}",
            self.uid, self.username, self.gid, self.group_name, groups
        )
    }
}

pub struct UserDatabase {
    users: BTreeMap<u32, User>,
    usernames: BTreeMap<String, u32>,
    groups: BTreeMap<u32, Group>,
}

impl UserDatabase {
    pub fn new() -> Self {
        let mut db = UserDatabase {
            users: BTreeMap::new(),
            usernames: BTreeMap::new(),
            groups: BTreeMap::new(),
        };
        db.seed_default_accounts();
        db
    }

    fn add_group(&mut self, group: Group) {
        self.groups.insert(group.gid, group);
    }

    fn add_user(&mut self, user: User) {
        self.usernames.insert(user.username.clone(), user.uid);
        self.users.insert(user.uid, user);
    }

    fn seed_default_accounts(&mut self) {
        self.add_group(Group::new(ROOT_GID, "root"));
        self.add_group(Group::new(SUDO_GID, "sudo"));
        self.add_group(Group::new(ADMIN_GID, "admin"));
        self.add_group(Group::new(GUEST_GID, "guest"));
        self.add_group(Group::new(WWW_DATA_GID, "www-data"));

        self.add_user(User::new(
            ROOT_UID,
            "root",
            ROOT_GID,
            vec![],
            VirtualPath::resolve(&VirtualPath::root(), "/root").unwrap(),
            "/bin/bash",
            PasswordState::Locked,
        ));
        self.add_user(User::new(
            ADMIN_UID,
            "admin",
            ADMIN_GID,
            vec![SUDO_GID],
            VirtualPath::resolve(&VirtualPath::root(), "/home/admin").unwrap(),
            "/bin/bash",
            PasswordState::set("admin123"),
        ));
        self.add_user(User::new(
            GUEST_UID,
            "guest",
            GUEST_GID,
            vec![],
            VirtualPath::resolve(&VirtualPath::root(), "/home/guest").unwrap(),
            "/bin/bash",
            PasswordState::set("guest"),
        ));
        self.add_user(User::new(
            WWW_DATA_UID,
            "www-data",
            WWW_DATA_GID,
            vec![],
            VirtualPath::resolve(&VirtualPath::root(), "/var/www").unwrap(),
            "/usr/sbin/nologin",
            PasswordState::Locked,
        ));
    }

    pub fn user_by_uid(&self, uid: u32) -> Option<&User> {
        self.users.get(&uid)
    }

    pub fn user_by_name(&self, username: &str) -> Option<&User> {
        self.usernames.get(username).and_then(|uid| self.users.get(uid))
    }

    pub fn group_by_gid(&self, gid: u32) -> Option<&Group> {
        self.groups.get(&gid)
    }

    pub fn group_by_name(&self, name: &str) -> Option<&Group> {
        self.groups.values().find(|g| g.name == name)
    }

    pub fn whoami(&self, uid: u32) -> Option<&str> {
        self.user_by_uid(uid).map(|u| u.username.as_str())
    }

    /// Primary group first, then supplementary groups, matching `id`/`groups` order.
    pub fn groups_of(&self, uid: u32) -> Option<Vec<&Group>> {
        let user = self.user_by_uid(uid)?;
        let mut groups = Vec::with_capacity(1 + user.supplementary_gids.len());
        if let Some(g) = self.group_by_gid(user.primary_gid) {
            groups.push(g);
        }
        for gid in &user.supplementary_gids {
            if let Some(g) = self.group_by_gid(*gid) {
                groups.push(g);
            }
        }
        Some(groups)
    }

    pub fn id_info(&self, uid: u32) -> Option<IdInfo> {
        let user = self.user_by_uid(uid)?;
        let group_name = self
            .group_by_gid(user.primary_gid)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let groups = self
            .groups_of(uid)?
            .into_iter()
            .map(|g| (g.gid, g.name.clone()))
            .collect();
        Some(IdInfo { uid, username: user.username.clone(), gid: user.primary_gid, group_name, groups })
    }

    pub fn verify_password(&self, uid: u32, attempt: &str) -> bool {
        self.user_by_uid(uid).is_some_and(|u| u.password.verify(attempt))
    }

    pub fn set_password(&mut self, uid: u32, new_password: &str) -> bool {
        match self.users.get_mut(&uid) {
            Some(user) => {
                user.password = PasswordState::set(new_password);
                true
            }
            None => false,
        }
    }

    /// Looks a user up by name and checks their password, as `login`/`su` would.
    pub fn authenticate(&self, username: &str, attempt: &str) -> Option<u32> {
        let user = self.user_by_name(username)?;
        user.password.verify(attempt).then_some(user.uid)
    }
}

impl Default for UserDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accounts_exist() {
        let db = UserDatabase::new();
        assert_eq!(db.whoami(0), Some("root"));
        assert_eq!(db.whoami(1000), Some("admin"));
        assert_eq!(db.whoami(1001), Some("guest"));
        assert_eq!(db.whoami(33), Some("www-data"));
        assert_eq!(db.whoami(9999), None);
    }

    #[test]
    fn lookup_by_name_matches_lookup_by_uid() {
        let db = UserDatabase::new();
        assert_eq!(db.user_by_name("guest").unwrap().uid, 1001);
    }

    #[test]
    fn guest_id_info_has_no_supplementary_groups() {
        let db = UserDatabase::new();
        let id = db.id_info(1001).unwrap();
        assert_eq!(id.to_string(), "uid=1001(guest) gid=1001(guest) groups=1001(guest)");
    }

    #[test]
    fn admin_id_info_includes_sudo_group() {
        let db = UserDatabase::new();
        let id = db.id_info(1000).unwrap();
        assert_eq!(id.to_string(), "uid=1000(admin) gid=1000(admin) groups=1000(admin),27(sudo)");
    }

    #[test]
    fn groups_of_lists_primary_group_first() {
        let db = UserDatabase::new();
        let groups = db.groups_of(1000).unwrap();
        assert_eq!(groups[0].name, "admin");
        assert_eq!(groups[1].name, "sudo");
    }

    #[test]
    fn root_password_is_locked() {
        let db = UserDatabase::new();
        assert!(!db.verify_password(0, ""));
        assert!(!db.verify_password(0, "root"));
    }

    #[test]
    fn authenticate_succeeds_with_correct_password_only() {
        let db = UserDatabase::new();
        assert_eq!(db.authenticate("guest", "guest"), Some(1001));
        assert_eq!(db.authenticate("guest", "wrong"), None);
        assert_eq!(db.authenticate("nobody", "anything"), None);
    }

    #[test]
    fn set_password_changes_future_verification() {
        let mut db = UserDatabase::new();
        assert!(db.verify_password(1001, "guest"));
        assert!(db.set_password(1001, "newpass"));
        assert!(!db.verify_password(1001, "guest"));
        assert!(db.verify_password(1001, "newpass"));
        assert!(!db.set_password(9999, "x"));
    }
}
