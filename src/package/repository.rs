use std::collections::BTreeMap;

use super::manifest::PackageManifest;

/// The simulated AGPKG catalog. Static and embedded — no real network
/// fetches, matching Section 16 ("do not implement real internet downloads").
pub struct Repository {
    packages: BTreeMap<String, PackageManifest>,
}

impl Repository {
    pub fn new() -> Self {
        let entries = [
            PackageManifest::new("openssl", "3.0.2", "TLS/SSL cryptography library", &[]),
            PackageManifest::new(
                "curl",
                "8.4.0",
                "Command line tool for transferring data with URLs",
                &["openssl"],
            ),
            PackageManifest::new("git", "2.42.0", "Distributed version control system", &[]),
            PackageManifest::new("vim", "9.0", "Vi IMproved text editor", &[]),
            PackageManifest::new("python3", "3.11.4", "Python 3 interpreter", &[]),
            PackageManifest::new("netcat", "1.10", "TCP/IP swiss army knife", &[]),
            PackageManifest::new(
                "nmap",
                "7.94",
                "Network exploration and security auditing tool",
                &["openssl"],
            ),
            PackageManifest::new("hydra", "9.4", "Fast network logon cracker", &["openssl"]),
        ];
        let packages = entries.into_iter().map(|p| (p.name.clone(), p)).collect();
        Repository { packages }
    }

    pub fn get(&self, name: &str) -> Option<&PackageManifest> {
        self.packages.get(name)
    }

    pub fn all(&self) -> Vec<&PackageManifest> {
        self.packages.values().collect()
    }

    pub fn search(&self, query: &str) -> Vec<&PackageManifest> {
        let query = query.to_lowercase();
        self.packages
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&query)
                    || p.description.to_lowercase().contains(&query)
            })
            .collect()
    }
}

impl Default for Repository {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_finds_a_known_package() {
        assert_eq!(Repository::new().get("nmap").unwrap().version, "7.94");
    }

    #[test]
    fn get_returns_none_for_unknown_package() {
        assert!(Repository::new().get("nope").is_none());
    }

    #[test]
    fn search_matches_name_or_description_case_insensitively() {
        let repo = Repository::new();
        assert!(repo.search("NMAP").iter().any(|p| p.name == "nmap"));
        assert!(repo.search("cracker").iter().any(|p| p.name == "hydra"));
        assert!(repo.search("nonexistent").is_empty());
    }
}
