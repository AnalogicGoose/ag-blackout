use super::knowledge::Knowledge;

/// The persistent player/progress layer that sits above a `Shell` session.
/// `Shell` is session-scoped (where the player currently is); `Knowledge` is
/// what the player has learned regardless of where they're currently
/// connected, so it lives here instead — see docs/GAME_DESIGN.md's Slice 2
/// section. `Economy`/`ContractBoard` staying on `Shell` for now, rather
/// than moving in here too, is a deliberate scope decision for Slice 2, not
/// an oversight — see docs/ARCHITECTURE.md.
#[derive(Default)]
pub struct Career {
    pub knowledge: Knowledge,
}

impl Career {
    pub fn new() -> Self {
        Self::default()
    }
}
