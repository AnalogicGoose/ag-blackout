use crate::career::{Contract, Objective};
use crate::filesystem::{FsAccess, Mode, VirtualPath};
use crate::world::{CredentialLead, Device, Network, Organization};

use super::session::Shell;

const PLAYER_HOST: &str = "localhost";

/// Every target's dedicated account uses this uid — safe to reuse across
/// devices since each `Device` owns an independent `UserDatabase`.
const TARGET_UID: u32 = 2000;

// Slice 2's investigative job: an `Organization` owning two devices. The
// seed device only has the default `guest`/`guest` account; a note left in
// its guest home directory leaks a reused password for the account that
// actually protects the target's resource. See docs/GAME_DESIGN.md's Slice 2
// section — this is the one-hop password-reuse chain it describes.
const GUIDED_ORG_NAME: &str = "Meridian Analytics";
const GUIDED_ORG_BLURB: &str =
    "A data analytics firm. Public records show it operates out of Managua, Nicaragua.";
const GUIDED_HINT: &str = "Operates in Managua, Nicaragua.";
const GUIDED_SEED_HOST: &str = "meridian-web01";
const GUIDED_TARGET_HOST: &str = "meridian-db01";
const GUIDED_NOTE_PATH: &str = "/home/guest/todo.txt";
const GUIDED_NOTE_CONTENT: &[u8] =
    b"TODO before the audit:\n- rotate the analyst account password (still 'M3ridian2024' on every box)\n- patch nginx\n";
const GUIDED_CREDENTIAL_USERNAME: &str = "analyst";
const GUIDED_CREDENTIAL_PASSWORD: &str = "M3ridian2024";
const GUIDED_RESOURCE_PATH: &str = "/home/analyst/customers.csv";
const GUIDED_RESOURCE_CONTENT: &[u8] = b"name,email,plan\nA. Vance,avance@example.com,enterprise\n";
const GUIDED_REWARD: i64 = 7500;
const PRIVILEGE_HOST: &str = "audit-vault01";
const PRIVILEGE_CONFIG_PATH: &str = "/etc/backup-agent.conf";
const PRIVILEGE_CONFIG_CONTENT: &[u8] =
    b"# Backup agent access\nuser=admin\npassword=R3cover2026!\n";
const PRIVILEGE_ADMIN_PASSWORD: &str = "R3cover2026!";
const PRIVILEGE_RESOURCE_PATH: &str = "/root/recovery.key";
const PRIVILEGE_RESOURCE_CONTENT: &[u8] = b"AG-RECOVERY-KEY-41\n";
const PRIVILEGE_REWARD: i64 = 9000;

/// What the player has to do to the resource, and what the target starts
/// out holding at that path.
enum Task {
    /// `download` this content back home.
    Obtain { content: &'static [u8] },
    /// The target starts with `initial_content`; the player must overwrite
    /// it (e.g. `echo ... > path` while connected) with `required_content`.
    Modify {
        initial_content: &'static [u8],
        required_content: &'static [u8],
    },
}

struct Job {
    hostname: &'static str,
    username: &'static str,
    password: &'static str,
    resource: &'static str,
    task: Task,
    title: &'static str,
    reward: i64,
}

const JOBS: &[Job] = &[
    Job {
        hostname: "corp-fs01",
        username: "dvance",
        password: "Q3report!",
        resource: "/home/dvance/report.pdf",
        task: Task::Obtain {
            content: b"Q3 financial report - CONFIDENTIAL\n",
        },
        title: "Retrieve the Q3 report",
        reward: 3000,
    },
    Job {
        hostname: "backup01",
        username: "opsbot",
        password: "backup-ok",
        resource: "/home/opsbot/backup.log",
        task: Task::Obtain {
            content: b"backup completed 2026-09-24 03:00 UTC, 812GB, 0 errors\n",
        },
        title: "Confirm the backup completed",
        reward: 1500,
    },
    Job {
        hostname: "sales-crm01",
        username: "crmuser",
        password: "sales2026",
        resource: "/home/crmuser/clients.csv",
        task: Task::Obtain {
            content: b"name,email,plan\nA. Vance,avance@example.com,enterprise\n",
        },
        title: "Pull the client list",
        reward: 4500,
    },
    Job {
        hostname: "monitor01",
        username: "svcacct",
        password: "watchdog1",
        resource: "/home/svcacct/status.txt",
        task: Task::Modify {
            initial_content: b"status: DEGRADED\n",
            required_content: b"status: OK\n",
        },
        title: "Fake the health check",
        reward: 2000,
    },
];

/// Boots the scenario: the player's own machine, one Directed target device
/// per `JOBS` entry, and the Slice 2 Guided investigation (an `Organization`
/// plus its two devices). Every contract is posted to the board as
/// `Available` — the player browses and accepts with `contracts`/`contracts
/// accept <id>`. Directed jobs work like Slice 1 always did: `connect` with
/// the handed-over login, then either `download` the resource home
/// (`Task::Obtain`) or overwrite it in place (`Task::Modify`). The Guided job
/// instead only names an `Organization` and a hint — `whois`/`scan`,
/// reading a leaked note, and reusing its credential are what actually get
/// the player there. This is content, not engine — see docs/GAME_DESIGN.md.
pub fn tutorial() -> Shell {
    let mut network = Network::new();
    network.register(Device::new(PLAYER_HOST));

    let mut shell = Shell::new(network, PLAYER_HOST, 1001); // logged in locally as guest

    for job in JOBS {
        let mut target = Device::new(job.hostname);

        let home = target
            .users
            .add_account(TARGET_UID, job.username, job.password);
        target.filesystem.mkdir(&FsAccess::root(), &home).unwrap();
        target
            .filesystem
            .chown(&FsAccess::root(), &home, TARGET_UID, TARGET_UID)
            .unwrap();
        target
            .filesystem
            .chmod(&FsAccess::root(), &home, Mode::new(0o700))
            .unwrap();

        let (seed_content, objective) = match job.task {
            Task::Obtain { content } => (content, Objective::ObtainResource),
            Task::Modify {
                initial_content,
                required_content,
            } => (
                initial_content,
                Objective::ModifyResource {
                    required_content: required_content.to_vec(),
                },
            ),
        };

        let resource_path = VirtualPath::resolve(&VirtualPath::root(), job.resource).unwrap();
        target
            .filesystem
            .write_file(&FsAccess::root(), &resource_path, seed_content)
            .unwrap();
        target
            .filesystem
            .chown(&FsAccess::root(), &resource_path, TARGET_UID, TARGET_UID)
            .unwrap();

        shell.network.register(target);
        shell.contracts.post(Contract::directed(
            job.title,
            job.hostname,
            resource_path,
            job.reward,
            job.username,
            job.password,
            objective,
        ));
    }

    setup_guided_investigation(&mut shell);
    setup_privilege_chain(&mut shell);

    shell
}

fn setup_guided_investigation(shell: &mut Shell) {
    shell.organizations.register(Organization::new(
        GUIDED_ORG_NAME,
        GUIDED_ORG_BLURB,
        vec![GUIDED_SEED_HOST.to_string(), GUIDED_TARGET_HOST.to_string()],
    ));

    let mut seed = Device::new(GUIDED_SEED_HOST);
    let note_path = VirtualPath::resolve(&VirtualPath::root(), GUIDED_NOTE_PATH).unwrap();
    seed.filesystem
        .write_file(&FsAccess::root(), &note_path, GUIDED_NOTE_CONTENT)
        .unwrap();
    seed.credential_leads.push(CredentialLead {
        path: note_path,
        username: GUIDED_CREDENTIAL_USERNAME.to_string(),
        password: GUIDED_CREDENTIAL_PASSWORD.to_string(),
    });
    shell.network.register(seed);

    let mut target = Device::new(GUIDED_TARGET_HOST);
    let home = target.users.add_account(
        TARGET_UID,
        GUIDED_CREDENTIAL_USERNAME,
        GUIDED_CREDENTIAL_PASSWORD,
    );
    target.filesystem.mkdir(&FsAccess::root(), &home).unwrap();
    target
        .filesystem
        .chown(&FsAccess::root(), &home, TARGET_UID, TARGET_UID)
        .unwrap();
    target
        .filesystem
        .chmod(&FsAccess::root(), &home, Mode::new(0o700))
        .unwrap();

    let resource_path = VirtualPath::resolve(&VirtualPath::root(), GUIDED_RESOURCE_PATH).unwrap();
    target
        .filesystem
        .write_file(&FsAccess::root(), &resource_path, GUIDED_RESOURCE_CONTENT)
        .unwrap();
    target
        .filesystem
        .chown(&FsAccess::root(), &resource_path, TARGET_UID, TARGET_UID)
        .unwrap();
    shell.network.register(target);

    shell.contracts.post(Contract::guided(
        "Obtain the customer database",
        GUIDED_TARGET_HOST,
        resource_path,
        GUIDED_REWARD,
        GUIDED_ORG_NAME,
        GUIDED_HINT,
        Objective::ObtainResource,
    ));
}

fn setup_privilege_chain(shell: &mut Shell) {
    let mut target = Device::new(PRIVILEGE_HOST);
    assert!(target.users.set_password(1000, PRIVILEGE_ADMIN_PASSWORD));

    let config_path = VirtualPath::resolve(&VirtualPath::root(), PRIVILEGE_CONFIG_PATH).unwrap();
    target
        .filesystem
        .write_file(&FsAccess::root(), &config_path, PRIVILEGE_CONFIG_CONTENT)
        .unwrap();
    target
        .filesystem
        .chmod(&FsAccess::root(), &config_path, Mode::new(0o644))
        .unwrap();
    target.credential_leads.push(CredentialLead {
        path: config_path,
        username: "admin".to_string(),
        password: PRIVILEGE_ADMIN_PASSWORD.to_string(),
    });

    let resource_path =
        VirtualPath::resolve(&VirtualPath::root(), PRIVILEGE_RESOURCE_PATH).unwrap();
    target
        .filesystem
        .write_file(
            &FsAccess::root(),
            &resource_path,
            PRIVILEGE_RESOURCE_CONTENT,
        )
        .unwrap();
    target
        .filesystem
        .chmod(&FsAccess::root(), &resource_path, Mode::new(0o600))
        .unwrap();
    let root_path = VirtualPath::resolve(&VirtualPath::root(), "/root").unwrap();
    target
        .filesystem
        .chmod(&FsAccess::root(), &root_path, Mode::new(0o700))
        .unwrap();

    shell.network.register(target);
    shell.contracts.post(Contract::directed(
        "Retrieve /root/recovery.key (inspect backup-agent config)",
        PRIVILEGE_HOST,
        resource_path,
        PRIVILEGE_REWARD,
        "guest",
        "guest",
        Objective::ObtainResource,
    ));
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
        assert_eq!(shell.contracts.available().count(), JOBS.len() + 2); // + the Slice 2 Guided contract
        assert_eq!(shell.contracts.active().count(), 0);
        assert_eq!(shell.economy.balance(), 0);
        assert!(shell.network.is_reachable(PRIVILEGE_HOST));
    }

    #[test]
    fn first_job_is_completable_after_accepting_it() {
        let mut shell = tutorial();
        let job = &JOBS[0];

        shell.execute_line("contracts accept 1");
        let result = shell.execute_line(&format!(
            "connect {} {} {}",
            job.hostname, job.username, job.password
        ));
        assert_eq!(result.exit_code, 0);

        let result = shell.execute_line(&format!("download {}", job.resource));
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), job.reward);
        assert_eq!(shell.contracts.completed().count(), 1);
    }

    #[test]
    fn downloading_the_resource_without_accepting_first_does_not_pay_out() {
        let mut shell = tutorial();
        let job = &JOBS[0];

        shell.execute_line(&format!(
            "connect {} {} {}",
            job.hostname, job.username, job.password
        ));
        shell.execute_line(&format!("download {}", job.resource));

        assert_eq!(shell.economy.balance(), 0);
        assert_eq!(shell.contracts.available().count(), JOBS.len() + 2);
    }

    #[test]
    fn modify_job_is_completable_by_overwriting_the_resource_in_place() {
        let mut shell = tutorial();
        let job = JOBS
            .iter()
            .find(|j| matches!(j.task, Task::Modify { .. }))
            .unwrap();
        let required_content = match job.task {
            Task::Modify {
                required_content, ..
            } => required_content,
            Task::Obtain { .. } => unreachable!(),
        };
        let id = shell
            .contracts
            .all()
            .iter()
            .find(|c| c.target_hostname == job.hostname)
            .unwrap()
            .id;

        shell.execute_line(&format!("contracts accept {id}"));
        shell.execute_line(&format!(
            "connect {} {} {}",
            job.hostname, job.username, job.password
        ));

        let write_command = format!(
            "echo {} > {}",
            String::from_utf8_lossy(required_content).trim_end(),
            job.resource
        );
        let result = shell.execute_line(&write_command);
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), job.reward);
        assert_eq!(shell.contracts.completed().count(), 1);
    }

    #[test]
    fn each_target_has_its_own_distinct_account() {
        let shell = tutorial();
        for job in JOBS {
            let target = shell.network.get(job.hostname).unwrap();
            assert!(
                target
                    .users
                    .authenticate(job.username, job.password)
                    .is_some()
            );
        }
    }

    fn guided_contract_id(shell: &Shell) -> u32 {
        shell
            .contracts
            .all()
            .iter()
            .find(|c| matches!(c.lead, crate::career::Lead::Guided { .. }))
            .unwrap()
            .id
    }

    #[test]
    fn guided_contract_briefing_never_discloses_the_target_hostname() {
        let mut shell = tutorial();
        let out = shell.execute_line("contracts").stdout;
        assert!(out.contains("Obtain the customer database"));
        assert!(out.contains(GUIDED_ORG_NAME));
        assert!(out.contains(GUIDED_HINT));
        assert!(!out.contains(GUIDED_TARGET_HOST));
    }

    #[test]
    fn guided_contract_is_completable_through_investigation() {
        let mut shell = tutorial();
        let id = guided_contract_id(&shell);
        shell.execute_line(&format!("contracts accept {id}"));

        // whois resolves the organization to its known infrastructure.
        let whois = shell.execute_line(&format!("whois {GUIDED_ORG_NAME}"));
        assert!(whois.stdout.contains(GUIDED_SEED_HOST));
        assert!(whois.stdout.contains(GUIDED_TARGET_HOST));

        // The seed device only needs the universal default account.
        shell.execute_line(&format!("connect {GUIDED_SEED_HOST} guest guest"));
        let note = shell.execute_line(&format!("cat {GUIDED_NOTE_PATH}"));
        assert_eq!(note.exit_code, 0);

        // Reading the note recorded the leaked credential into Knowledge.
        let credential = shell
            .career
            .knowledge
            .credentials()
            .iter()
            .find(|c| c.username == GUIDED_CREDENTIAL_USERNAME)
            .expect("credential should have been recorded");
        assert_eq!(credential.password, GUIDED_CREDENTIAL_PASSWORD);
        assert_eq!(credential.found_on, GUIDED_SEED_HOST);

        shell.execute_line("disconnect");

        // The reused credential is what actually gets the player onto the
        // device holding the resource.
        let connect = shell.execute_line(&format!(
            "connect {GUIDED_TARGET_HOST} {GUIDED_CREDENTIAL_USERNAME} {GUIDED_CREDENTIAL_PASSWORD}"
        ));
        assert_eq!(connect.exit_code, 0);

        let result = shell.execute_line(&format!("download {GUIDED_RESOURCE_PATH}"));
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), GUIDED_REWARD);
        assert_eq!(shell.contracts.completed().count(), 1);
    }

    #[test]
    fn the_default_guest_account_cannot_reach_the_guided_resource_directly() {
        let mut shell = tutorial();
        shell.execute_line(&format!("connect {GUIDED_TARGET_HOST} guest guest"));
        let result = shell.execute_line(&format!("download {GUIDED_RESOURCE_PATH}"));
        assert_ne!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), 0);
    }

    #[test]
    fn privilege_contract_requires_discovery_and_sudo_download() {
        let mut shell = tutorial();
        let id = shell
            .contracts
            .all()
            .iter()
            .find(|contract| contract.target_hostname == PRIVILEGE_HOST)
            .unwrap()
            .id;

        assert_eq!(
            shell
                .execute_line(&format!("contracts accept {id}"))
                .exit_code,
            0
        );
        assert_eq!(
            shell
                .execute_line(&format!("connect {PRIVILEGE_HOST} guest guest"))
                .exit_code,
            0
        );

        assert_ne!(
            shell
                .execute_line(&format!("cat {PRIVILEGE_RESOURCE_PATH}"))
                .exit_code,
            0
        );
        assert_ne!(
            shell
                .execute_line(&format!("download {PRIVILEGE_RESOURCE_PATH}"))
                .exit_code,
            0
        );
        assert_ne!(shell.execute_line("su admin admin123").exit_code, 0);
        assert_eq!(shell.economy.balance(), 0);

        let config = shell.execute_line(&format!("cat {PRIVILEGE_CONFIG_PATH}"));
        assert_eq!(config.exit_code, 0);
        assert!(config.stdout.contains(PRIVILEGE_ADMIN_PASSWORD));

        let credential = shell
            .career
            .knowledge
            .credentials()
            .iter()
            .find(|credential| {
                credential.username == "admin" && credential.found_on == PRIVILEGE_HOST
            })
            .expect("reading the config should record the credential");
        assert_eq!(credential.password, PRIVILEGE_ADMIN_PASSWORD);

        assert_eq!(
            shell
                .execute_line(&format!("su admin {PRIVILEGE_ADMIN_PASSWORD}"))
                .exit_code,
            0
        );
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
        assert_ne!(
            shell
                .execute_line(&format!("download {PRIVILEGE_RESOURCE_PATH}"))
                .exit_code,
            0
        );

        let sudo_command = format!("sudo download {PRIVILEGE_RESOURCE_PATH}");
        assert_ne!(shell.execute_sudo(&sudo_command, "wrong").exit_code, 0);
        assert_eq!(shell.economy.balance(), 0);

        assert_eq!(
            shell
                .execute_sudo(&sudo_command, PRIVILEGE_ADMIN_PASSWORD)
                .exit_code,
            0
        );
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
        assert_eq!(shell.economy.balance(), PRIVILEGE_REWARD);
        assert_eq!(shell.contracts.completed().count(), 1);

        // Repetir la descarga no debe volver a pagar.
        assert_eq!(
            shell
                .execute_sudo(&sudo_command, PRIVILEGE_ADMIN_PASSWORD)
                .exit_code,
            0
        );
        assert_eq!(shell.economy.balance(), PRIVILEGE_REWARD);

        shell.execute_line("disconnect");
        assert_eq!(
            shell.execute_line("cat /home/guest/recovery.key").stdout,
            "AG-RECOVERY-KEY-41\n"
        );
    }
}
