use std::collections::BTreeMap;

use thiserror::Error;

use super::process::ProcessTable;

const WWW_DATA_UID: u32 = 33;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ServiceError {
    #[error("unknown service: {0}")]
    NotFound(String),
    #[error("permission denied (root required)")]
    NotPermitted,
    #[error("service already running")]
    AlreadyRunning,
    #[error("service already stopped")]
    AlreadyStopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct Service {
    pub name: String,
    pub command: String,
    pub owner_uid: u32,
    pub state: ServiceState,
    pub pid: Option<u32>,
    /// Flavor/informational only for now — the smallest change needed for
    /// `scan` to show "nginx 1.18.0" once `nmap` is installed. See
    /// docs/GAME_DESIGN.md's Slice 2 section: a version is information, not
    /// a vulnerability check.
    pub version: String,
}

/// Services as system state (Section 14): starting/stopping one starts/stops
/// its backing `Process`, both gated to root — this is not a real daemon
/// manager, just enough state for `service`/`ps` to be meaningful together.
pub struct ServiceRegistry {
    services: BTreeMap<String, Service>,
}

impl ServiceRegistry {
    pub fn new(processes: &mut ProcessTable) -> Self {
        let mut registry = ServiceRegistry {
            services: BTreeMap::new(),
        };
        registry.seed_running(processes, "sshd", "sshd", 0, "9.3");
        registry.seed_running(processes, "cron", "cron", 0, "3.0pl1");
        registry.seed_running(processes, "nginx", "nginx", WWW_DATA_UID, "1.18.0");
        registry
    }

    fn seed_running(
        &mut self,
        processes: &mut ProcessTable,
        name: &str,
        command: &str,
        owner_uid: u32,
        version: &str,
    ) {
        let pid = processes.spawn(1, owner_uid, command);
        self.services.insert(
            name.to_string(),
            Service {
                name: name.to_string(),
                command: command.to_string(),
                owner_uid,
                state: ServiceState::Running,
                pid: Some(pid),
                version: version.to_string(),
            },
        );
    }

    pub fn get(&self, name: &str) -> Option<&Service> {
        self.services.get(name)
    }

    pub fn list(&self) -> Vec<&Service> {
        self.services.values().collect()
    }

    /// Changes the installed service version in world state. This is an
    /// internal simulation hook; no player command patches services yet.
    pub fn set_version(
        &mut self,
        name: &str,
        version: impl Into<String>,
    ) -> Result<(), ServiceError> {
        let service = self
            .services
            .get_mut(name)
            .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;
        service.version = version.into();
        Ok(())
    }

    pub fn stop(
        &mut self,
        processes: &mut ProcessTable,
        is_root: bool,
        name: &str,
    ) -> Result<(), ServiceError> {
        if !is_root {
            return Err(ServiceError::NotPermitted);
        }
        let service = self
            .services
            .get_mut(name)
            .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;
        if service.state == ServiceState::Stopped {
            return Err(ServiceError::AlreadyStopped);
        }
        if let Some(pid) = service.pid.take() {
            processes.remove(pid);
        }
        service.state = ServiceState::Stopped;
        Ok(())
    }

    pub fn start(
        &mut self,
        processes: &mut ProcessTable,
        is_root: bool,
        name: &str,
    ) -> Result<(), ServiceError> {
        if !is_root {
            return Err(ServiceError::NotPermitted);
        }
        let (owner_uid, command) = {
            let service = self
                .services
                .get(name)
                .ok_or_else(|| ServiceError::NotFound(name.to_string()))?;
            if service.state == ServiceState::Running {
                return Err(ServiceError::AlreadyRunning);
            }
            (service.owner_uid, service.command.clone())
        };
        let pid = processes.spawn(1, owner_uid, command);
        let service = self.services.get_mut(name).expect("just looked up above");
        service.pid = Some(pid);
        service.state = ServiceState::Running;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_services_are_running_with_backing_processes() {
        let mut processes = ProcessTable::new();
        let registry = ServiceRegistry::new(&mut processes);
        let sshd = registry.get("sshd").unwrap();
        assert_eq!(sshd.state, ServiceState::Running);
        assert!(processes.get(sshd.pid.unwrap()).is_some());
    }

    #[test]
    fn stop_requires_root() {
        let mut processes = ProcessTable::new();
        let mut registry = ServiceRegistry::new(&mut processes);
        assert_eq!(
            registry.stop(&mut processes, false, "sshd").unwrap_err(),
            ServiceError::NotPermitted
        );
    }

    #[test]
    fn stop_removes_the_backing_process() {
        let mut processes = ProcessTable::new();
        let mut registry = ServiceRegistry::new(&mut processes);
        let pid = registry.get("sshd").unwrap().pid.unwrap();
        registry.stop(&mut processes, true, "sshd").unwrap();
        assert_eq!(registry.get("sshd").unwrap().state, ServiceState::Stopped);
        assert!(processes.get(pid).is_none());
    }

    #[test]
    fn cannot_stop_an_already_stopped_service() {
        let mut processes = ProcessTable::new();
        let mut registry = ServiceRegistry::new(&mut processes);
        registry.stop(&mut processes, true, "sshd").unwrap();
        assert_eq!(
            registry.stop(&mut processes, true, "sshd").unwrap_err(),
            ServiceError::AlreadyStopped
        );
    }

    #[test]
    fn start_spawns_a_fresh_process() {
        let mut processes = ProcessTable::new();
        let mut registry = ServiceRegistry::new(&mut processes);
        let old_pid = registry.get("sshd").unwrap().pid.unwrap();
        registry.stop(&mut processes, true, "sshd").unwrap();
        registry.start(&mut processes, true, "sshd").unwrap();
        let new_pid = registry.get("sshd").unwrap().pid.unwrap();
        assert_ne!(old_pid, new_pid);
        assert_eq!(registry.get("sshd").unwrap().state, ServiceState::Running);
    }

    #[test]
    fn unknown_service_errors() {
        let mut processes = ProcessTable::new();
        let mut registry = ServiceRegistry::new(&mut processes);
        assert_eq!(
            registry.start(&mut processes, true, "nope").unwrap_err(),
            ServiceError::NotFound("nope".to_string())
        );
    }
}
