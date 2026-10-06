use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    /// False if this was pulled in only as someone else's dependency —
    /// mirrors apt's "automatically installed" flag, shown by `agpkg list`.
    pub explicit: bool,
}

/// What `/var/lib/agpkg/installed.db` would represent. Starts empty — a
/// fresh AG Linux install has no AGPKG packages, only the builtins the shell
/// already provides.
#[derive(Default)]
pub struct InstalledDatabase {
    installed: BTreeMap<String, InstalledPackage>,
}

impl InstalledDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_installed(&self, name: &str) -> bool {
        self.installed.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&InstalledPackage> {
        self.installed.get(name)
    }

    pub fn list(&self) -> Vec<&InstalledPackage> {
        self.installed.values().collect()
    }

    pub fn insert(&mut self, package: InstalledPackage) {
        self.installed.insert(package.name.clone(), package);
    }

    pub fn remove(&mut self, name: &str) -> Option<InstalledPackage> {
        self.installed.remove(name)
    }

    pub fn set_version(&mut self, name: &str, version: String) {
        if let Some(pkg) = self.installed.get_mut(name) {
            pkg.version = version;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        assert!(InstalledDatabase::new().list().is_empty());
    }

    #[test]
    fn insert_and_lookup() {
        let mut db = InstalledDatabase::new();
        db.insert(InstalledPackage {
            name: "nmap".to_string(),
            version: "7.94".to_string(),
            explicit: true,
        });
        assert!(db.is_installed("nmap"));
        assert_eq!(db.get("nmap").unwrap().version, "7.94");
    }

    #[test]
    fn remove_drops_the_entry() {
        let mut db = InstalledDatabase::new();
        db.insert(InstalledPackage {
            name: "nmap".to_string(),
            version: "7.94".to_string(),
            explicit: true,
        });
        db.remove("nmap");
        assert!(!db.is_installed("nmap"));
    }

    #[test]
    fn set_version_updates_in_place() {
        let mut db = InstalledDatabase::new();
        db.insert(InstalledPackage {
            name: "curl".to_string(),
            version: "7.0".to_string(),
            explicit: true,
        });
        db.set_version("curl", "8.4.0".to_string());
        assert_eq!(db.get("curl").unwrap().version, "8.4.0");
    }
}
