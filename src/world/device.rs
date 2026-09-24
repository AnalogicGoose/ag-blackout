use crate::filesystem::{VirtualFS, VirtualPath};
use crate::package::PackageManager;
use crate::system::{LogBook, ProcessTable, ServiceRegistry, Sudoers, UserDatabase};

/// A credential a device's content deliberately reveals once the exact
/// `path` is read — a sticky note, a leaked config. This is content, not a
/// special "loot" mechanic: the engine's only job is copying it into the
/// player's `Knowledge` once that exact path is actually read (see
/// `Shell::note_credential_leads` and docs/GAME_DESIGN.md's Slice 2
/// section). Nothing about `VirtualFS` itself changes — the file is a
/// perfectly ordinary one.
#[derive(Clone, Debug)]
pub struct CredentialLead {
    pub path: VirtualPath,
    pub username: String,
    pub password: String,
}

/// A machine on the network. Right now this directly holds an AG Linux
/// instance (filesystem/users/processes/services/packages/logs) since AG
/// Linux is the only implemented OS — see docs/GAME_DESIGN.md. When a second
/// OS family becomes real, these fields are what gets pulled out behind an
/// OS trait/enum; there's no reason to build that seam for one implementor.
pub struct Device {
    pub hostname: String,
    pub filesystem: VirtualFS,
    pub users: UserDatabase,
    pub sudoers: Sudoers,
    pub processes: ProcessTable,
    pub services: ServiceRegistry,
    pub packages: PackageManager,
    pub logs: LogBook,
    pub credential_leads: Vec<CredentialLead>,
}

impl Device {
    pub fn new(hostname: impl Into<String>) -> Self {
        let mut processes = ProcessTable::new();
        let services = ServiceRegistry::new(&mut processes);
        Device {
            hostname: hostname.into(),
            filesystem: VirtualFS::new(),
            users: UserDatabase::new(),
            sudoers: Sudoers::new(),
            processes,
            services,
            packages: PackageManager::new(),
            logs: LogBook::default(),
            credential_leads: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_device_boots_a_fresh_ag_linux_instance() {
        let device = Device::new("target01");
        assert_eq!(device.hostname, "target01");
        assert!(device.users.whoami(0).is_some()); // root exists
        assert!(device.services.get("sshd").is_some());
    }
}
