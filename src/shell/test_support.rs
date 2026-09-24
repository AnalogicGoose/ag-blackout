use crate::world::{Device, Network};

use super::session::Shell;

pub fn shell_for(uid: u32) -> Shell {
    let mut network = Network::new();
    network.register(Device::new("localhost"));
    Shell::new(network, "localhost", uid)
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
