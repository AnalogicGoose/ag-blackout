/// The player's money. Whole-dollar integers — no need for fractional
/// currency at this stage.
#[derive(Default)]
pub struct Economy {
    balance: i64,
}

impl Economy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn balance(&self) -> i64 {
        self.balance
    }

    pub fn deposit(&mut self, amount: i64) {
        self.balance += amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero_and_accumulates_deposits() {
        let mut economy = Economy::new();
        assert_eq!(economy.balance(), 0);
        economy.deposit(3000);
        economy.deposit(500);
        assert_eq!(economy.balance(), 3500);
    }
}
