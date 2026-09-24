use crate::filesystem::VirtualPath;

use super::contract::{Contract, ContractStatus};

/// The player's accepted contracts. Resolution is a simple synchronous
/// check — no event bus — matching docs/GAME_DESIGN.md: the CLI calls
/// `record_read` after a qualifying action (currently: `cat`) and gets back
/// whichever contracts that action just completed.
#[derive(Default)]
pub struct ContractBoard {
    contracts: Vec<Contract>,
}

impl ContractBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn accept(&mut self, contract: Contract) {
        self.contracts.push(contract);
    }

    pub fn active(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.iter().filter(|c| c.status == ContractStatus::Active)
    }

    pub fn all(&self) -> &[Contract] {
        &self.contracts
    }

    /// Marks any active contract targeting `(hostname, path)` as completed.
    /// Returns the contracts that were just completed, so the caller can pay
    /// out their reward.
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

    #[test]
    fn record_read_completes_a_matching_contract() {
        let mut board = ContractBoard::new();
        board.accept(Contract::new("Get the report", "target01", path("/home/finance/report.pdf"), 3000));

        let completed = board.record_read("target01", &path("/home/finance/report.pdf"));
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].reward, 3000);
        assert_eq!(board.active().count(), 0);
    }

    #[test]
    fn record_read_ignores_non_matching_reads() {
        let mut board = ContractBoard::new();
        board.accept(Contract::new("Get the report", "target01", path("/home/finance/report.pdf"), 3000));

        assert!(board.record_read("target01", &path("/etc/hostname")).is_empty());
        assert!(board.record_read("other-host", &path("/home/finance/report.pdf")).is_empty());
        assert_eq!(board.active().count(), 1);
    }

    #[test]
    fn completed_contract_does_not_resolve_twice() {
        let mut board = ContractBoard::new();
        board.accept(Contract::new("Get the report", "target01", path("/home/finance/report.pdf"), 3000));
        board.record_read("target01", &path("/home/finance/report.pdf"));

        assert!(board.record_read("target01", &path("/home/finance/report.pdf")).is_empty());
    }
}
