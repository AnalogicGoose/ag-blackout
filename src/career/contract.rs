use crate::filesystem::VirtualPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractStatus {
    /// Posted to the board, not yet taken.
    Available,
    Active,
    Completed,
}

/// What "satisfying" a contract means. Two kinds so far; shaped so a future
/// kind (DeleteResource/GainAccess/...) slots in beside them without
/// touching `Contract`'s other fields or how contracts are stored. See
/// docs/GAME_DESIGN.md.
#[derive(Clone, Debug)]
pub enum Objective {
    /// Copy `resource_path` off the target back to the player's own device
    /// via `download`. See `ContractBoard::record_download`.
    ObtainResource,
    /// Overwrite `resource_path` on the target so its bytes exactly match
    /// `required_content` — e.g. `echo ... > path` while connected. See
    /// `ContractBoard::record_modify`.
    ModifyResource { required_content: Vec<u8> },
}

/// How much of the path to a contract's target the player is handed up
/// front. `target_hostname` on `Contract` is always the engine's concrete
/// resolution target, known from authoring time for both kinds — `lead` only
/// controls what `contracts` actually *discloses* to the player. See
/// docs/GAME_DESIGN.md's Slice 2 section.
#[derive(Clone, Debug)]
pub enum Lead {
    /// Hostname and login handed to the player directly — Slice 1's shape.
    Directed { username: String, password: String },
    /// Only an `Organization` name and a hint are handed over; the player
    /// has to discover the actual device (and a working credential for it)
    /// through investigation (`whois`/`scan`) and gameplay (reading files,
    /// reusing a harvested password) before `connect` can ever reach it.
    Guided { organization: String, hint: String },
}

#[derive(Clone, Debug)]
pub struct Contract {
    /// Assigned by `ContractBoard::post`; `0` until then.
    pub id: u32,
    pub title: String,
    pub target_hostname: String,
    pub resource_path: VirtualPath,
    pub reward: i64,
    pub status: ContractStatus,
    pub objective: Objective,
    pub lead: Lead,
}

impl Contract {
    /// A contract that hands the player the target hostname and login
    /// outright.
    pub fn directed(
        title: impl Into<String>,
        target_hostname: impl Into<String>,
        resource_path: VirtualPath,
        reward: i64,
        username: impl Into<String>,
        password: impl Into<String>,
        objective: Objective,
    ) -> Self {
        Contract {
            id: 0,
            title: title.into(),
            target_hostname: target_hostname.into(),
            resource_path,
            reward,
            status: ContractStatus::Available,
            objective,
            lead: Lead::Directed {
                username: username.into(),
                password: password.into(),
            },
        }
    }

    /// A contract that hands the player only an `Organization` and a hint —
    /// `target_hostname` still names the real device the engine resolves
    /// against, but `contracts` won't disclose it; see `Lead::Guided`.
    pub fn guided(
        title: impl Into<String>,
        target_hostname: impl Into<String>,
        resource_path: VirtualPath,
        reward: i64,
        organization: impl Into<String>,
        hint: impl Into<String>,
        objective: Objective,
    ) -> Self {
        Contract {
            id: 0,
            title: title.into(),
            target_hostname: target_hostname.into(),
            resource_path,
            reward,
            status: ContractStatus::Available,
            objective,
            lead: Lead::Guided {
                organization: organization.into(),
                hint: hint.into(),
            },
        }
    }
}
