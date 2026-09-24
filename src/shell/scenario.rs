use crate::career::Contract;
use crate::filesystem::{ FsAccess, VirtualPath };
use crate::world::{ Device, Network };

use super::session::Shell;

const PLAYER_HOST: &str = "localhost";

struct Job {
    hostname: &'static str,
    resource: &'static str,
    content: &'static [u8],
    title: &'static str,
    reward: i64,
}

const JOBS: &[Job] = &[
    Job {
        hostname: "corp-fs01",
        resource: "/home/guest/report.pdf",
        content: b"Q3 financial report - CONFIDENTIAL\n",
        title: "Retrieve the Q3 report",
        reward: 3000,
    },
    Job {
        hostname: "backup01",
        resource: "/home/guest/backup.log",
        content: b"backup completed 2026-09-24 03:00 UTC, 812GB, 0 errors\n",
        title: "Confirm the backup completed",
        reward: 1500,
    },
    Job {
        hostname: "sales-crm01",
        resource: "/home/guest/clients.csv",
        content: b"name,email,plan\nA. Vance,avance@example.com,enterprise\n",
        title: "Pull the client list",
        reward: 4500,
    },
];

/// Boots the Slice 1 scenario: the player's own machine plus one target
/// device per `JOBS` entry, each holding its resource, with all three
/// contracts posted to the board as `Available` — the player browses and
/// accepts with `contracts`/`contracts accept <id>`, then works them like
/// any other job. This is content, not engine — see docs/GAME_DESIGN.md.
/// Credentials are handed over rather than discovered (matches Slice 1's
/// proof-of-access design); every target reuses its default `guest` account
/// since `UserDatabase` has no public way to add a distinct one yet.
pub fn tutorial() -> Shell {
    let mut network = Network::new();
    network.register(Device::new(PLAYER_HOST));

    let mut shell = Shell::new(network, PLAYER_HOST, 1001); // logged in locally as guest

    for job in JOBS {
        let mut target = Device::new(job.hostname);
        let resource_path = VirtualPath::resolve(&VirtualPath::root(), job.resource).unwrap();
        target.filesystem.write_file(&FsAccess::root(), &resource_path, job.content).unwrap();
        shell.network.register(target);
        shell.contracts.post(Contract::new(job.title, job.hostname, resource_path, job.reward));
    }

    shell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_scenario_registers_every_target_and_posts_every_contract() {
        let shell = tutorial();
        for job in JOBS {
            assert!(shell.network.is_reachable(job.hostname));
        }
        assert_eq!(shell.contracts.available().count(), JOBS.len());
        assert_eq!(shell.contracts.active().count(), 0);
        assert_eq!(shell.economy.balance(), 0);
    }

    #[test]
    fn first_job_is_completable_after_accepting_it() {
        let mut shell = tutorial();
        let job = &JOBS[0];

        shell.execute_line("contracts accept 1");
        let result = shell.execute_line(&format!("connect {} guest guest", job.hostname));
        assert_eq!(result.exit_code, 0);

        let result = shell.execute_line(&format!("cat {}", job.resource));
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), job.reward);
        assert_eq!(shell.contracts.completed().count(), 1);
    }

    #[test]
    fn reading_the_resource_without_accepting_first_does_not_pay_out() {
        let mut shell = tutorial();
        let job = &JOBS[0];

        shell.execute_line(&format!("connect {} guest guest", job.hostname));
        shell.execute_line(&format!("cat {}", job.resource));

        assert_eq!(shell.economy.balance(), 0);
        assert_eq!(shell.contracts.available().count(), JOBS.len());
    }
}