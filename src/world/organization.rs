use std::collections::HashMap;

/// A named entity that owns a set of devices — a grouping layer on top of
/// `Network`, not a duplicate registry. `Network` remains the only place
/// `Device`s actually live; `Organization` just remembers which hostnames
/// belong to it, for `whois` and `Contract::Lead::Guided` to reference. See
/// docs/GAME_DESIGN.md's Slice 2 section.
#[derive(Clone, Debug)]
pub struct Organization {
    pub name: String,
    pub blurb: String,
    pub hostnames: Vec<String>,
}

impl Organization {
    pub fn new(name: impl Into<String>, blurb: impl Into<String>, hostnames: Vec<String>) -> Self {
        Organization {
            name: name.into(),
            blurb: blurb.into(),
            hostnames,
        }
    }

    pub fn owns(&self, hostname: &str) -> bool {
        self.hostnames.iter().any(|h| h == hostname)
    }
}

/// Looked up by name — mirrors `Network`'s `hostname -> Device` shape at the
/// organization level, kept as its own registry rather than folded into
/// `Network` (see docs/GAME_DESIGN.md: no organization-specific logic in
/// `Network`).
#[derive(Default)]
pub struct OrganizationRegistry {
    organizations: HashMap<String, Organization>,
}

impl OrganizationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, organization: Organization) {
        self.organizations
            .insert(organization.name.clone(), organization);
    }

    pub fn get(&self, name: &str) -> Option<&Organization> {
        self.organizations.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_checks_membership() {
        let org = Organization::new(
            "Meridian Analytics",
            "A data firm.",
            vec!["corp-web01".to_string()],
        );
        assert!(org.owns("corp-web01"));
        assert!(!org.owns("corp-db01"));
    }

    #[test]
    fn registry_looks_up_by_name() {
        let mut registry = OrganizationRegistry::new();
        registry.register(Organization::new(
            "Meridian Analytics",
            "A data firm.",
            vec!["corp-web01".to_string()],
        ));
        assert!(registry.get("Meridian Analytics").is_some());
        assert!(registry.get("Nope Corp").is_none());
    }
}
