use std::collections::HashMap;

use crate::filesystem::{FsAccess, VirtualPath};

use super::group::Group;
use super::user::User;

/// What a command actually runs against: identity, cwd and environment.
/// Built once per login/`su`/`sudo` from a `User`, then threaded through the
/// shell into `Filesystem`/`System` calls — see ARCHITECTURE.md section 6.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub uid: u32,
    pub gids: Vec<u32>,
    pub cwd: VirtualPath,
    pub env: HashMap<String, String>,
}

impl ExecutionContext {
    pub fn for_user(user: &User, groups: &[&Group]) -> Self {
        let mut gids: Vec<u32> = std::iter::once(user.primary_gid)
            .chain(groups.iter().map(|g| g.gid))
            .collect();
        gids.dedup();

        let mut env = HashMap::new();
        env.insert("HOME".to_string(), user.home.to_string());
        env.insert("USER".to_string(), user.username.clone());
        env.insert("SHELL".to_string(), user.shell.clone());
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );

        ExecutionContext {
            uid: user.uid,
            gids,
            cwd: user.home.clone(),
            env,
        }
    }

    pub fn fs_access(&self) -> FsAccess {
        FsAccess::new(self.uid, self.gids.clone())
    }

    pub fn is_root(&self) -> bool {
        self.uid == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::user::PasswordState;

    #[test]
    fn built_from_user_carries_identity_and_groups() {
        let group = Group::new(27, "sudo");
        let user = User::new(
            1000,
            "admin",
            1000,
            vec![27],
            VirtualPath::resolve(&VirtualPath::root(), "/home/admin").unwrap(),
            "/bin/bash",
            PasswordState::set("admin123"),
        );
        let ctx = ExecutionContext::for_user(&user, &[&group]);

        assert_eq!(ctx.uid, 1000);
        assert_eq!(ctx.gids, vec![1000, 27]);
        assert_eq!(ctx.cwd.to_string(), "/home/admin");
        assert_eq!(ctx.env.get("USER"), Some(&"admin".to_string()));
        assert_eq!(ctx.env.get("HOME"), Some(&"/home/admin".to_string()));
        assert!(!ctx.is_root());
    }

    #[test]
    fn fs_access_mirrors_uid_and_groups() {
        let user = User::new(
            1001,
            "guest",
            1001,
            vec![],
            VirtualPath::resolve(&VirtualPath::root(), "/home/guest").unwrap(),
            "/bin/bash",
            PasswordState::set("guest"),
        );
        let ctx = ExecutionContext::for_user(&user, &[]);
        let access = ctx.fs_access();

        assert_eq!(access.uid, 1001);
        assert_eq!(access.gids, vec![1001]);
        assert!(!access.is_superuser);
    }

    #[test]
    fn root_context_is_superuser_via_fs_access() {
        let user = User::new(
            0,
            "root",
            0,
            vec![],
            VirtualPath::resolve(&VirtualPath::root(), "/root").unwrap(),
            "/bin/bash",
            PasswordState::Locked,
        );
        let ctx = ExecutionContext::for_user(&user, &[]);
        assert!(ctx.is_root());
        assert!(ctx.fs_access().is_superuser);
    }
}
