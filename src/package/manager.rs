use thiserror::Error;

use super::installed::{InstalledDatabase, InstalledPackage};
use super::manifest::PackageManifest;
use super::repository::Repository;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PackageError {
    #[error("permission denied (root required)")]
    NotPermitted,
    #[error("unable to locate package {0}")]
    NotFound(String),
    #[error("{0} is already the newest version")]
    AlreadyInstalled(String),
    #[error("package {0} is not installed")]
    NotInstalled(String),
    #[error("cannot remove {0}: required by {1}")]
    RequiredBy(String, String),
}

/// The AGPKG engine: a static `Repository` (the catalog) plus an
/// `InstalledDatabase` (what's actually on this machine). `install`/`remove`/
/// `update`/`upgrade` all require root, matching real package managers.
pub struct PackageManager {
    pub repository: Repository,
    pub installed: InstalledDatabase,
}

impl PackageManager {
    pub fn new() -> Self {
        PackageManager {
            repository: Repository::new(),
            installed: InstalledDatabase::new(),
        }
    }

    pub fn search(&self, query: &str) -> Vec<&PackageManifest> {
        self.repository.search(query)
    }

    pub fn info(&self, name: &str) -> Option<&PackageManifest> {
        self.repository.get(name)
    }

    pub fn list_installed(&self) -> Vec<&InstalledPackage> {
        self.installed.list()
    }

    /// Installs `name` and any dependencies it needs, skipping ones already
    /// present (including diamond dependencies pulled in twice). Returns the
    /// names actually installed, in dependency-first order.
    pub fn install(&mut self, is_root: bool, name: &str) -> Result<Vec<String>, PackageError> {
        if !is_root {
            return Err(PackageError::NotPermitted);
        }
        if self.repository.get(name).is_none() {
            return Err(PackageError::NotFound(name.to_string()));
        }
        if self.installed.is_installed(name) {
            return Err(PackageError::AlreadyInstalled(name.to_string()));
        }

        let mut order = Vec::new();
        let mut visiting = Vec::new();
        self.resolve_deps(name, &mut order, &mut visiting)?;

        let mut newly_installed = Vec::new();
        for pkg_name in &order {
            if self.installed.is_installed(pkg_name) {
                continue;
            }
            let manifest = self
                .repository
                .get(pkg_name)
                .expect("resolved from the repository")
                .clone();
            self.installed.insert(InstalledPackage {
                name: manifest.name,
                version: manifest.version,
                explicit: pkg_name == name,
            });
            newly_installed.push(pkg_name.clone());
        }
        Ok(newly_installed)
    }

    fn resolve_deps(
        &self,
        name: &str,
        order: &mut Vec<String>,
        visiting: &mut Vec<String>,
    ) -> Result<(), PackageError> {
        if order.contains(&name.to_string()) || visiting.contains(&name.to_string()) {
            return Ok(());
        }
        visiting.push(name.to_string());
        let manifest = self
            .repository
            .get(name)
            .ok_or_else(|| PackageError::NotFound(name.to_string()))?;
        for dep in &manifest.dependencies {
            self.resolve_deps(dep, order, visiting)?;
        }
        order.push(name.to_string());
        visiting.pop();
        Ok(())
    }

    /// Blocked if another installed package still depends on `name` — we
    /// don't cascade-remove dependents like real package managers might.
    pub fn remove(&mut self, is_root: bool, name: &str) -> Result<(), PackageError> {
        if !is_root {
            return Err(PackageError::NotPermitted);
        }
        if !self.installed.is_installed(name) {
            return Err(PackageError::NotInstalled(name.to_string()));
        }
        if let Some(dependent) = self.find_dependent(name) {
            return Err(PackageError::RequiredBy(name.to_string(), dependent));
        }
        self.installed.remove(name);
        Ok(())
    }

    fn find_dependent(&self, name: &str) -> Option<String> {
        self.installed
            .list()
            .into_iter()
            .find(|pkg| {
                self.repository
                    .get(&pkg.name)
                    .map(|m| m.dependencies.iter().any(|d| d == name))
                    .unwrap_or(false)
            })
            .map(|pkg| pkg.name.clone())
    }

    /// Flavor only — the catalog is static and embedded, so there's nothing
    /// to actually refresh (Section 16: no real downloads).
    pub fn update(&self, is_root: bool) -> Result<(), PackageError> {
        if !is_root {
            return Err(PackageError::NotPermitted);
        }
        Ok(())
    }

    /// Bumps any installed package whose version doesn't match the catalog's
    /// current version. Returns the names actually upgraded.
    pub fn upgrade(&mut self, is_root: bool) -> Result<Vec<String>, PackageError> {
        if !is_root {
            return Err(PackageError::NotPermitted);
        }
        let mut upgraded = Vec::new();
        let names: Vec<String> = self
            .installed
            .list()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        for name in names {
            let Some(manifest) = self.repository.get(&name) else {
                continue;
            };
            let current_version = self
                .installed
                .get(&name)
                .expect("just listed")
                .version
                .clone();
            if current_version != manifest.version {
                self.installed.set_version(&name, manifest.version.clone());
                upgraded.push(name);
            }
        }
        Ok(upgraded)
    }
}

impl Default for PackageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_requires_root() {
        let mut pm = PackageManager::new();
        assert_eq!(
            pm.install(false, "nmap").unwrap_err(),
            PackageError::NotPermitted
        );
    }

    #[test]
    fn install_unknown_package_fails() {
        let mut pm = PackageManager::new();
        assert_eq!(
            pm.install(true, "nope").unwrap_err(),
            PackageError::NotFound("nope".to_string())
        );
    }

    #[test]
    fn install_pulls_in_dependencies() {
        let mut pm = PackageManager::new();
        let installed = pm.install(true, "nmap").unwrap();
        assert_eq!(installed, vec!["openssl".to_string(), "nmap".to_string()]);
        assert!(pm.installed.is_installed("openssl"));
        assert!(!pm.installed.get("openssl").unwrap().explicit);
        assert!(pm.installed.get("nmap").unwrap().explicit);
    }

    #[test]
    fn installing_twice_fails_the_second_time() {
        let mut pm = PackageManager::new();
        pm.install(true, "git").unwrap();
        assert_eq!(
            pm.install(true, "git").unwrap_err(),
            PackageError::AlreadyInstalled("git".to_string())
        );
    }

    #[test]
    fn shared_dependency_is_only_installed_once() {
        let mut pm = PackageManager::new();
        pm.install(true, "nmap").unwrap();
        let installed = pm.install(true, "hydra").unwrap();
        assert_eq!(installed, vec!["hydra".to_string()]); // openssl already there
    }

    #[test]
    fn remove_requires_root() {
        let mut pm = PackageManager::new();
        pm.install(true, "git").unwrap();
        assert_eq!(
            pm.remove(false, "git").unwrap_err(),
            PackageError::NotPermitted
        );
    }

    #[test]
    fn remove_not_installed_fails() {
        let mut pm = PackageManager::new();
        assert_eq!(
            pm.remove(true, "git").unwrap_err(),
            PackageError::NotInstalled("git".to_string())
        );
    }

    #[test]
    fn remove_blocked_while_a_dependent_is_installed() {
        let mut pm = PackageManager::new();
        pm.install(true, "nmap").unwrap();
        assert_eq!(
            pm.remove(true, "openssl").unwrap_err(),
            PackageError::RequiredBy("openssl".to_string(), "nmap".to_string())
        );
        pm.remove(true, "nmap").unwrap();
        assert!(pm.remove(true, "openssl").is_ok());
    }

    #[test]
    fn update_requires_root_and_otherwise_succeeds() {
        let pm = PackageManager::new();
        assert_eq!(pm.update(false).unwrap_err(), PackageError::NotPermitted);
        assert!(pm.update(true).is_ok());
    }

    #[test]
    fn upgrade_bumps_outdated_installed_versions() {
        let mut pm = PackageManager::new();
        pm.install(true, "curl").unwrap();
        pm.installed.set_version("curl", "7.0".to_string()); // simulate a stale install
        let upgraded = pm.upgrade(true).unwrap();
        assert_eq!(upgraded, vec!["curl".to_string()]);
        assert_eq!(pm.installed.get("curl").unwrap().version, "8.4.0");
    }

    #[test]
    fn upgrade_with_nothing_outdated_upgrades_nothing() {
        let mut pm = PackageManager::new();
        pm.install(true, "git").unwrap();
        assert!(pm.upgrade(true).unwrap().is_empty());
    }
}
