use std::collections::{BTreeMap, BTreeSet};

/// A credential the player has actually discovered during play, and where
/// they found it — as opposed to one merely handed over by a `Directed`
/// contract, which never touches `Knowledge` at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredCredential {
    pub username: String,
    pub password: String,
    pub found_on: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceObservation {
    pub hostname: String,
    pub name: String,
    pub version: String,
    pub running: bool,
}

/// What the player has learned about the world, independent of where
/// they're currently connected — persists across `connect`/`disconnect`,
/// unlike `Shell`'s session state. See docs/GAME_DESIGN.md's Slice 2
/// section: `whois`/`scan` record hostnames and organization affiliations;
/// an nmap-enhanced `scan` also records service observations. Reading an
/// authored credential-bearing file records a credential.
#[derive(Default)]
pub struct Knowledge {
    hostnames: BTreeSet<String>,
    organizations: BTreeMap<String, BTreeSet<String>>,
    services: BTreeMap<(String, String), ServiceObservation>,
    credentials: Vec<DiscoveredCredential>,
}

impl Knowledge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_hostname(&mut self, hostname: impl Into<String>) {
        self.hostnames.insert(hostname.into());
    }

    /// Also records `hostname` itself — an organization affiliation implies
    /// the hostname is known.
    pub fn record_organization(
        &mut self,
        organization: impl Into<String>,
        hostname: impl Into<String>,
    ) {
        let hostname = hostname.into();
        self.hostnames.insert(hostname.clone());
        self.organizations
            .entry(organization.into())
            .or_default()
            .insert(hostname);
    }

    pub fn record_credential(
        &mut self,
        username: impl Into<String>,
        password: impl Into<String>,
        found_on: impl Into<String>,
    ) {
        self.credentials.push(DiscoveredCredential {
            username: username.into(),
            password: password.into(),
            found_on: found_on.into(),
        });
    }

    pub fn record_service(
        &mut self,
        hostname: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        running: bool,
    ) {
        let hostname = hostname.into();
        let name = name.into();
        self.hostnames.insert(hostname.clone());
        self.services.insert(
            (hostname.clone(), name.clone()),
            ServiceObservation {
                hostname,
                name,
                version: version.into(),
                running,
            },
        );
    }

    pub fn knows_hostname(&self, hostname: &str) -> bool {
        self.hostnames.contains(hostname)
    }

    pub fn hostnames(&self) -> impl Iterator<Item = &str> {
        self.hostnames.iter().map(String::as_str)
    }

    pub fn hostnames_for_organization(&self, organization: &str) -> impl Iterator<Item = &str> {
        self.organizations
            .get(organization)
            .into_iter()
            .flat_map(|hosts| hosts.iter().map(String::as_str))
    }

    pub fn organizations(&self) -> impl Iterator<Item = (&str, &BTreeSet<String>)> {
        self.organizations
            .iter()
            .map(|(name, hosts)| (name.as_str(), hosts))
    }

    pub fn services(&self) -> impl Iterator<Item = &ServiceObservation> {
        self.services.values()
    }

    pub fn credentials(&self) -> &[DiscoveredCredential] {
        &self.credentials
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let k = Knowledge::new();
        assert!(k.hostnames().next().is_none());
        assert!(k.credentials().is_empty());
        assert!(!k.knows_hostname("corp-web01"));
    }

    #[test]
    fn record_hostname_is_remembered() {
        let mut k = Knowledge::new();
        k.record_hostname("corp-web01");
        assert!(k.knows_hostname("corp-web01"));
        assert!(!k.knows_hostname("corp-db01"));
    }

    #[test]
    fn record_organization_remembers_both_the_org_and_the_hostname() {
        let mut k = Knowledge::new();
        k.record_organization("Meridian Analytics", "corp-web01");
        k.record_organization("Meridian Analytics", "corp-db01");

        assert!(k.knows_hostname("corp-web01"));
        assert!(k.knows_hostname("corp-db01"));
        let hosts: Vec<_> = k.hostnames_for_organization("Meridian Analytics").collect();
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains(&"corp-web01"));
        assert!(hosts.contains(&"corp-db01"));
    }

    #[test]
    fn unknown_organization_yields_no_hostnames() {
        let k = Knowledge::new();
        assert_eq!(k.hostnames_for_organization("nope").count(), 0);
    }

    #[test]
    fn record_credential_is_remembered_with_its_source() {
        let mut k = Knowledge::new();
        k.record_credential("analyst", "hunter2", "corp-web01");
        assert_eq!(k.credentials().len(), 1);
        let cred = &k.credentials()[0];
        assert_eq!(cred.username, "analyst");
        assert_eq!(cred.password, "hunter2");
        assert_eq!(cred.found_on, "corp-web01");
    }

    #[test]
    fn repeated_service_observation_updates_in_place() {
        let mut knowledge = Knowledge::new();
        knowledge.record_service("web01", "nginx", "1.18.0", true);
        knowledge.record_service("web01", "nginx", "1.24.0", false);

        let observations = knowledge.services().collect::<Vec<_>>();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].version, "1.24.0");
        assert!(!observations[0].running);
        assert!(knowledge.knows_hostname("web01"));
    }
}
