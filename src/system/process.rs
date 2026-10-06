use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProcessError {
    #[error("no such process: {0}")]
    NotFound(u32),
    #[error("operation not permitted")]
    NotPermitted,
    #[error("init (pid 1) may not be killed")]
    CannotKillInit,
}

#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub command: String,
}

/// Simulated processes — game objects, not real OS processes. PID 1 is
/// `ag-init`, seeded once and never killable, matching real init/systemd.
pub struct ProcessTable {
    processes: BTreeMap<u32, Process>,
    next_pid: u32,
}

impl ProcessTable {
    pub fn new() -> Self {
        let mut table = ProcessTable {
            processes: BTreeMap::new(),
            next_pid: 1,
        };
        let init_pid = table.next_pid();
        table.processes.insert(
            init_pid,
            Process {
                pid: init_pid,
                ppid: 0,
                uid: 0,
                command: "ag-init".to_string(),
            },
        );
        table
    }

    fn next_pid(&mut self) -> u32 {
        let pid = self.next_pid;
        self.next_pid += 1;
        pid
    }

    pub fn spawn(&mut self, ppid: u32, uid: u32, command: impl Into<String>) -> u32 {
        let pid = self.next_pid();
        self.processes.insert(
            pid,
            Process {
                pid,
                ppid,
                uid,
                command: command.into(),
            },
        );
        pid
    }

    pub fn get(&self, pid: u32) -> Option<&Process> {
        self.processes.get(&pid)
    }

    pub fn list(&self) -> Vec<&Process> {
        self.processes.values().collect()
    }

    /// Removes a process with no permission check — for privileged callers
    /// like `ServiceRegistry`. User-facing code should use `kill` instead.
    pub fn remove(&mut self, pid: u32) -> Option<Process> {
        self.processes.remove(&pid)
    }

    /// `kill(1)` requires: the process must exist, may not be PID 1, and the
    /// caller must own it or be root — same as real Unix `kill(2)`.
    pub fn kill(&mut self, uid: u32, is_root: bool, pid: u32) -> Result<Process, ProcessError> {
        let process = self.get(pid).ok_or(ProcessError::NotFound(pid))?;
        if pid == 1 {
            return Err(ProcessError::CannotKillInit);
        }
        if !is_root && process.uid != uid {
            return Err(ProcessError::NotPermitted);
        }
        Ok(self.remove(pid).expect("just confirmed it exists"))
    }
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_init_as_pid_one() {
        let table = ProcessTable::new();
        let init = table.get(1).unwrap();
        assert_eq!(init.command, "ag-init");
        assert_eq!(init.uid, 0);
    }

    #[test]
    fn spawn_assigns_increasing_pids() {
        let mut table = ProcessTable::new();
        let a = table.spawn(1, 0, "sshd");
        let b = table.spawn(1, 0, "cron");
        assert!(b > a);
    }

    #[test]
    fn cannot_kill_init() {
        let mut table = ProcessTable::new();
        assert_eq!(
            table.kill(0, true, 1).unwrap_err(),
            ProcessError::CannotKillInit
        );
    }

    #[test]
    fn kill_unknown_pid_fails() {
        let mut table = ProcessTable::new();
        assert_eq!(
            table.kill(0, true, 999).unwrap_err(),
            ProcessError::NotFound(999)
        );
    }

    #[test]
    fn owner_can_kill_their_own_process() {
        let mut table = ProcessTable::new();
        let pid = table.spawn(1, 1001, "bash");
        assert!(table.kill(1001, false, pid).is_ok());
        assert!(table.get(pid).is_none());
    }

    #[test]
    fn non_owner_cannot_kill_unless_root() {
        let mut table = ProcessTable::new();
        let pid = table.spawn(1, 0, "sshd");
        assert_eq!(
            table.kill(1001, false, pid).unwrap_err(),
            ProcessError::NotPermitted
        );
        assert!(table.kill(0, true, pid).is_ok());
    }
}
