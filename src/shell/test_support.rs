use crate::filesystem::VirtualFS;
use crate::package::PackageManager;
use crate::system::{ProcessTable, ServiceRegistry, Sudoers, UserDatabase};

use super::session::Shell;

pub fn shell_for(uid: u32) -> Shell {
    let users = UserDatabase::new();
    let context = users.execution_context_for(uid).unwrap();
    let mut processes = ProcessTable::new();
    let services = ServiceRegistry::new(&mut processes);
    Shell::new(VirtualFS::new(), users, Sudoers::new(), context, processes, services, PackageManager::new())
}

pub fn guest_shell() -> Shell {
    shell_for(1001)
}

pub fn root_shell() -> Shell {
    shell_for(0)
}

pub fn admin_shell() -> Shell {
    shell_for(1000)
}
