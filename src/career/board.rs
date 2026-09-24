use thiserror::Error;

use crate::filesystem::VirtualPath;

use super::contract::{ Contract, ContractStatus };

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContractError {
    #[error("no such contract: {0}")]
    NotFound(u32),
    #[error("contract {0} is not available")]
    NotAvailable(u32),
}

/// The board: contracts posted, accepted, and completed. Resolution is a
/// simple synchronous check — no event bus — matching docs/GAME_DESIGN.md:
/// the CLI calls `record_read` after a qualifying action (currently: `cat`)
/// and gets back whichever contracts that action just completed.
#[derive(Default)]
pub struct ContractBoard {
    contracts: Vec<Contract>,
    next_id: u32,
}

impl ContractBoard {
    pub fn new() -> Self {
        ContractBoard { contracts: Vec::new(), next_id: 1 }
    }

    /// Adds a contract to the board as `Available`, assigning it an id.
    /// Returns the id so the caller (content setup, tests) can reference it.
    pub fn post(&mut self, mut contract: Contract) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        contract.id = id;
        contract.status = ContractStatus::Available;
        self.contracts.push(contract);
        id
    }

    pub fn accept(&mut self, id: u32) -> Result<(), ContractError> {
        let contract = self.contracts.iter_mut().find(|c| c.id == id).ok_or(ContractError::NotFound(id))?;
        if contract.status != ContractStatus::Available {
            return Err(ContractError::NotAvailable(id));
        }
        contract.status = ContractStatus::Active;
        Ok(())
    }

    pub fn available(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.iter().filter(|c| c.status == ContractStatus::Available)
    }

    pub fn active(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.iter().filter(|c| c.status == ContractStatus::Active)
    }

    pub fn completed(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.iter().filter(|c| c.status == ContractStatus::Completed)
    }

    pub fn all(&self) -> &[Contract] {
        &self.contracts
    }

    /// Marks any *active* contract targeting `(hostname, path)` as
    /// completed. Returns the contracts that were just completed, so the
    /// caller can pay out their reward.
    pub fn record_read(&mut self, hostname: &str, path: &VirtualPath) -> Vec<Contract> {
        let mut completed = Vec::new();
        for contract in self.contracts.iter_mut() {
            if contract.status == ContractStatus::Active && contract.target_hostname == hostname && &contract.resource_path == path {
                contract.status = ContractStatus::Completed;
                completed.push(contract.clone());
            }
        }
        completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> VirtualPath {
        VirtualPath::resolve(&VirtualPath::root(), s).unwrap()
    }

    fn sample() -> Contract {
        Contract::new("Get the report", "target01", path("/home/finance/report.pdf"), 3000)
    }

    #[test]
    fn posted_contract_starts_available_with_an_assigned_id() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        assert_eq!(id, 1);
        assert_eq!(board.available().count(), 1);
        assert_eq!(board.active().count(), 0);
    }

    #[test]
    fn ids_increase_across_posts() {
        let mut board = ContractBoard::new();
        let a = board.post(sample());
        let b = board.post(sample());
        assert_ne!(a, b);
    }

    #[test]
    fn accept_moves_a_contract_from_available_to_active() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        board.accept(id).unwrap();
        assert_eq!(board.available().count(), 0);
        assert_eq!(board.active().count(), 1);
    }

    #[test]
    fn accept_unknown_id_fails() {
        let mut board = ContractBoard::new();
        assert_eq!(board.accept(999).unwrap_err(), ContractError::NotFound(999));
    }

    #[test]
    fn accept_twice_fails_the_second_time() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        board.accept(id).unwrap();
        assert_eq!(board.accept(id).unwrap_err(), ContractError::NotAvailable(id));
    }

    #[test]
    fn record_read_only_resolves_active_contracts() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        // still just Available, not accepted yet — must not resolve
        assert!(board.record_read("target01", &path("/home/finance/report.pdf")).is_empty());

        board.accept(id).unwrap();
        let completed = board.record_read("target01", &path("/home/finance/report.pdf"));
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].reward, 3000);
        assert_eq!(board.active().count(), 0);
        assert_eq!(board.completed().count(), 1);
    }

    #[test]
    fn record_read_ignores_non_matching_reads() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        board.accept(id).unwrap();

        assert!(board.record_read("target01", &path("/etc/hostname")).is_empty());
        assert!(board.record_read("other-host", &path("/home/finance/report.pdf")).is_empty());
        assert_eq!(board.active().count(), 1);
    }

    #[test]
    fn completed_contract_does_not_resolve_twice() {
        let mut board = ContractBoard::new();
        let id = board.post(sample());
        board.accept(id).unwrap();
        board.record_read("target01", &path("/home/finance/report.pdf"));

        assert!(board.record_read("target01", &path("/home/finance/report.pdf")).is_empty());
    }
}
