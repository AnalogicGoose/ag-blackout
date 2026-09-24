use crate::filesystem::VirtualFS;
use crate::system::{Sudoers, UserDatabase};

use super::session::Shell;

pub fn shell_for(uid: u32) -> Shell {
    let users = UserDatabase::new();
    let context = users.execution_context_for(uid).unwrap();
    Shell::new(VirtualFS::new(), users, Sudoers::new(), context)
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
