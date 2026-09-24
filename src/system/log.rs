use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub source: String,
    pub message: String,
}

/// In-memory system log. Not persisted to `/var/log` yet — no simulated
/// clock exists to timestamp entries meaningfully in-world, and nothing
/// reads log files back off disk yet either. Revisit if that changes.
#[derive(Default)]
pub struct LogBook {
    entries: Vec<LogEntry>,
}

impl LogBook {
    pub fn record(&mut self, source: impl Into<String>, message: impl Into<String>) {
        self.entries.push(LogEntry { timestamp: SystemTime::now(), source: source.into(), message: message.into() });
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_entries_in_order() {
        let mut log = LogBook::default();
        log.record("kill", "sshd (pid 2) terminated by uid 0");
        log.record("service", "nginx stopped by uid 0");
        let entries = log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "kill");
        assert_eq!(entries[1].message, "nginx stopped by uid 0");
    }
}
