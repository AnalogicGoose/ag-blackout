use crate::career::Contract;
use crate::filesystem::{ FsAccess, VirtualPath };
use crate::world::{ Device, Network };

use super::session::Shell;

const PLAYER_HOST: &str = "localhost";
const TARGET_HOST: &str = "corp-fs01";
const TARGET_RESOURCE: &str = "/home/guest/report.pdf";
const CONTRACT_REWARD: i64 = 3000;

/// Boots the Slice 1 tutorial scenario: the player's own machine plus one
/// target device holding a resource, with a contract already accepted for
/// it. This is content, not engine — see docs/GAME_DESIGN.md. Credentials
/// are handed over rather than discovered (matches Slice 1's proof-of-access
/// design); it reuses the target's default `guest` account since
/// `UserDatabase` has no public way to add a distinct one yet.
pub fn tutorial() -> Shell {
    let mut network = Network::new();
    network.register(Device::new(PLAYER_HOST));

    let mut target = Device::new(TARGET_HOST);
    let resource_path = VirtualPath::resolve(&VirtualPath::root(), TARGET_RESOURCE).unwrap();
    target.filesystem.write_file(&FsAccess::root(), &resource_path, b"Q3 financial report - CONFIDENTIAL\n").unwrap();
    network.register(target);

    let mut shell = Shell::new(network, PLAYER_HOST, 1001); // logged in locally as guest
    shell.contracts.accept(Contract::new("Retrieve the Q3 report", TARGET_HOST, resource_path, CONTRACT_REWARD));
    shell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_scenario_registers_target_and_accepts_one_contract() {
        let shell = tutorial();
        assert!(shell.network.is_reachable(TARGET_HOST));
        assert_eq!(shell.contracts.active().count(), 1);
        assert_eq!(shell.economy.balance(), 0);
    }

    #[test]
    fn tutorial_scenario_is_completable_with_the_handed_over_credentials() {
        let mut shell = tutorial();
        let result = shell.execute_line(&format!("connect {TARGET_HOST} guest guest"));
        assert_eq!(result.exit_code, 0);

        let result = shell.execute_line(&format!("cat {TARGET_RESOURCE}"));
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), CONTRACT_REWARD);
        assert_eq!(shell.contracts.active().count(), 0);
    }
}