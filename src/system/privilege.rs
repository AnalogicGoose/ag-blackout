use thiserror::Error;

use super::context::ExecutionContext;
use super::registry::UserDatabase;
use super::sudoers::Sudoers;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PrivilegeError {
    #[error("{0} is not in the sudoers file.")]
    NotPermitted(String),
    #[error("Sorry, try again.")]
    IncorrectPassword,
    #[error("unknown user: {0}")]
    UnknownUser(String),
}

/// `su [-] <user>` — switch identity. Proves knowledge of the *target*
/// account's password, unless the caller is already root (root can `su` to
/// anyone with no password, same as real Unix). `login` mirrors the `-`/
/// `--login` flag: true resets cwd/env to the target's login shell, false
/// (plain `su`) keeps the caller's current directory.
pub fn su(
    db: &UserDatabase,
    current: &ExecutionContext,
    target_username: &str,
    password: Option<&str>,
    login: bool,
) -> Result<ExecutionContext, PrivilegeError> {
    let target = db
        .user_by_name(target_username)
        .ok_or_else(|| PrivilegeError::UnknownUser(target_username.to_string()))?;

    if !current.is_root() {
        let attempt = password.ok_or(PrivilegeError::IncorrectPassword)?;
        if !target.password.verify(attempt) {
            return Err(PrivilegeError::IncorrectPassword);
        }
    }

    let mut ctx = db
        .execution_context_for(target.uid)
        .expect("target user was just looked up");
    if !login {
        ctx.cwd = current.cwd.clone();
    }
    Ok(ctx)
}

/// `sudo [-u <user>] <command>` — run as another user (root by default) after
/// checking sudoers membership and the *caller's own* password (not the
/// target's — that's the whole point of sudo over su). Keeps the caller's cwd,
/// since sudo runs one command in place rather than switching login sessions.
pub fn sudo(
    db: &UserDatabase,
    sudoers: &Sudoers,
    current: &ExecutionContext,
    caller_password: &str,
    target_username: Option<&str>,
) -> Result<ExecutionContext, PrivilegeError> {
    let caller = db
        .user_by_uid(current.uid)
        .ok_or_else(|| PrivilegeError::UnknownUser(current.uid.to_string()))?;

    if !current.is_root() {
        if !sudoers.permits(db, current.uid) {
            return Err(PrivilegeError::NotPermitted(caller.username.clone()));
        }
        if !caller.password.verify(caller_password) {
            return Err(PrivilegeError::IncorrectPassword);
        }
    }

    let target_name = target_username.unwrap_or("root");
    let target = db
        .user_by_name(target_name)
        .ok_or_else(|| PrivilegeError::UnknownUser(target_name.to_string()))?;

    let mut ctx = db
        .execution_context_for(target.uid)
        .expect("target user was just looked up");
    ctx.cwd = current.cwd.clone();
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn su_from_root_needs_no_password() {
        let db = UserDatabase::new();
        let root_ctx = db.execution_context_for(0).unwrap();
        let ctx = su(&db, &root_ctx, "guest", None, false).unwrap();
        assert_eq!(ctx.uid, 1001);
    }

    #[test]
    fn su_requires_correct_target_password() {
        let db = UserDatabase::new();
        let guest_ctx = db.execution_context_for(1001).unwrap();
        assert!(su(&db, &guest_ctx, "admin", Some("admin123"), false).is_ok());
        assert_eq!(
            su(&db, &guest_ctx, "admin", Some("wrong"), false).unwrap_err(),
            PrivilegeError::IncorrectPassword
        );
        assert_eq!(
            su(&db, &guest_ctx, "admin", None, false).unwrap_err(),
            PrivilegeError::IncorrectPassword
        );
    }

    #[test]
    fn su_unknown_user_fails() {
        let db = UserDatabase::new();
        let root_ctx = db.execution_context_for(0).unwrap();
        assert_eq!(
            su(&db, &root_ctx, "nobody", None, false).unwrap_err(),
            PrivilegeError::UnknownUser("nobody".to_string())
        );
    }

    #[test]
    fn su_without_login_flag_keeps_caller_cwd() {
        let db = UserDatabase::new();
        let root_ctx = db.execution_context_for(0).unwrap();
        let ctx = su(&db, &root_ctx, "guest", None, false).unwrap();
        assert_eq!(ctx.cwd.to_string(), "/root");
    }

    #[test]
    fn su_with_login_flag_uses_target_home() {
        let db = UserDatabase::new();
        let root_ctx = db.execution_context_for(0).unwrap();
        let ctx = su(&db, &root_ctx, "guest", None, true).unwrap();
        assert_eq!(ctx.cwd.to_string(), "/home/guest");
    }

    #[test]
    fn sudo_denies_non_sudoer() {
        let db = UserDatabase::new();
        let sudoers = Sudoers::new();
        let guest_ctx = db.execution_context_for(1001).unwrap();
        assert_eq!(
            sudo(&db, &sudoers, &guest_ctx, "guest", None).unwrap_err(),
            PrivilegeError::NotPermitted("guest".to_string())
        );
    }

    #[test]
    fn sudo_allows_sudo_group_member_with_correct_password() {
        let db = UserDatabase::new();
        let sudoers = Sudoers::new();
        let admin_ctx = db.execution_context_for(1000).unwrap();
        let ctx = sudo(&db, &sudoers, &admin_ctx, "admin123", None).unwrap();
        assert_eq!(ctx.uid, 0); // default target is root
    }

    #[test]
    fn sudo_wrong_password_fails_even_for_sudoer() {
        let db = UserDatabase::new();
        let sudoers = Sudoers::new();
        let admin_ctx = db.execution_context_for(1000).unwrap();
        assert_eq!(
            sudo(&db, &sudoers, &admin_ctx, "wrong", None).unwrap_err(),
            PrivilegeError::IncorrectPassword
        );
    }

    #[test]
    fn sudo_from_root_never_needs_password_or_sudoers() {
        let db = UserDatabase::new();
        let sudoers = Sudoers::new();
        let root_ctx = db.execution_context_for(0).unwrap();
        let ctx = sudo(&db, &sudoers, &root_ctx, "", Some("guest")).unwrap();
        assert_eq!(ctx.uid, 1001);
    }

    #[test]
    fn sudo_keeps_callers_cwd() {
        let db = UserDatabase::new();
        let sudoers = Sudoers::new();
        let admin_ctx = db.execution_context_for(1000).unwrap();
        let ctx = sudo(&db, &sudoers, &admin_ctx, "admin123", None).unwrap();
        assert_eq!(ctx.cwd.to_string(), "/home/admin");
    }
}
