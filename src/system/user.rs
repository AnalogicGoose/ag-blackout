use crate::filesystem::VirtualPath;

/// Not real cryptography — this is a gameplay simulation. FNV-1a keeps
/// `/etc/shadow` looking like a hash (and crackable later, Phase 7) instead
/// of storing passwords in the clear.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn hash_password(password: &str) -> String {
    format!("$agfnv1${:016x}", fnv1a(password.as_bytes()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasswordState {
    /// shadow field is "!" — no interactive login (service accounts, and
    /// root until Phase 3 wires up `sudo`/`su`).
    Locked,
    Set(String),
}

impl PasswordState {
    pub fn set(password: &str) -> Self {
        PasswordState::Set(hash_password(password))
    }

    pub fn verify(&self, attempt: &str) -> bool {
        match self {
            PasswordState::Locked => false,
            PasswordState::Set(hash) => *hash == hash_password(attempt),
        }
    }
}

#[derive(Clone, Debug)]
pub struct User {
    pub uid: u32,
    pub username: String,
    pub primary_gid: u32,
    pub supplementary_gids: Vec<u32>,
    pub home: VirtualPath,
    pub shell: String,
    pub password: PasswordState,
}

impl User {
    pub fn new(
        uid: u32,
        username: impl Into<String>,
        primary_gid: u32,
        supplementary_gids: Vec<u32>,
        home: VirtualPath,
        shell: impl Into<String>,
        password: PasswordState,
    ) -> Self {
        User {
            uid,
            username: username.into(),
            primary_gid,
            supplementary_gids,
            home,
            shell: shell.into(),
            password,
        }
    }

    pub fn is_root(&self) -> bool {
        self.uid == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_password_hashes_the_same() {
        assert_eq!(hash_password("hunter2"), hash_password("hunter2"));
    }

    #[test]
    fn different_passwords_hash_differently() {
        assert_ne!(hash_password("hunter2"), hash_password("hunter3"));
    }

    #[test]
    fn locked_password_never_verifies() {
        assert!(!PasswordState::Locked.verify(""));
        assert!(!PasswordState::Locked.verify("anything"));
    }

    #[test]
    fn set_password_verifies_only_correct_attempt() {
        let pw = PasswordState::set("hunter2");
        assert!(pw.verify("hunter2"));
        assert!(!pw.verify("hunter3"));
    }
}
