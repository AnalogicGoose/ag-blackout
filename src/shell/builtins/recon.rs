use crate::system::ServiceState;

use super::super::output::CommandOutput;
use super::super::session::Shell;

/// Free and always available — resolves an `Organization` name to its known
/// infrastructure (see docs/GAME_DESIGN.md's Slice 2 section). Joins all
/// args with a space so a multi-word organization name doesn't need to be
/// quoted on the command line.
pub fn whois(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    if args.is_empty() {
        return CommandOutput::error("whois: usage: whois <organization>\n");
    }
    let name = args.join(" ");
    let Some(org) = shell.organizations.get(&name) else {
        return CommandOutput::error(format!("whois: no public record for '{name}'\n"));
    };
    let (org_name, blurb, hostnames) = (org.name.clone(), org.blurb.clone(), org.hostnames.clone());

    let mut out = format!("{org_name}\n{blurb}\nKnown infrastructure:\n");
    for host in &hostnames {
        out.push_str(&format!("  - {host}\n"));
    }

    for host in &hostnames {
        shell
            .career
            .knowledge
            .record_organization(org_name.clone(), host.clone());
    }

    CommandOutput::ok(out)
}

/// Reachability always; with `nmap` installed on the device the player is
/// currently sitting at, also lists the target's services and versions —
/// information, never a verdict (no "vulnerable" flag). See
/// docs/GAME_DESIGN.md's Slice 2 section.
pub fn scan(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let [hostname] = args else {
        return CommandOutput::error("scan: usage: scan <hostname>\n");
    };
    if !shell.network.is_reachable(hostname) {
        return CommandOutput::ok(format!("{hostname}: host unreachable\n"));
    }

    shell.career.knowledge.record_hostname(hostname.clone());

    let mut out = format!("{hostname}: host is up\n");
    if shell
        .active_device()
        .packages
        .installed
        .is_installed("nmap")
    {
        let services = {
            let device = shell
                .network
                .get(hostname)
                .expect("just checked reachability");
            device
                .services
                .list()
                .into_iter()
                .map(|service| {
                    (
                        service.name.clone(),
                        service.version.clone(),
                        service.state == ServiceState::Running,
                    )
                })
                .collect::<Vec<_>>()
        };

        for (name, version, running) in services {
            let state = if running { "running" } else { "stopped" };
            out.push_str(&format!("  {name} {version} ({state})\n"));
            shell
                .career
                .knowledge
                .record_service(hostname.clone(), name, version, running);
        }
    }

    CommandOutput::ok(out)
}

pub fn exploit(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let [hostname, service] = args else {
        return CommandOutput::error("exploit: usage: exploit <hostname> <service>\n");
    };
    match shell.exploit_service(hostname, service) {
        Ok(()) => CommandOutput::ok(format!("Session opened on {hostname}.\n")),
        Err(error) => CommandOutput::error(format!("exploit: {error}\n")),
    }
}

/// Reviews what `Career::knowledge` has accumulated so far — discovered
/// hosts, credentials, and service observations.
pub fn intel(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let knowledge = &shell.career.knowledge;
    let mut out = String::new();

    let mut hostnames: Vec<&str> = knowledge.hostnames().collect();
    if !hostnames.is_empty() {
        hostnames.sort();
        out.push_str("Known hosts:\n");
        for host in hostnames {
            out.push_str(&format!("  {host}\n"));
        }
    }

    let credentials = knowledge.credentials();
    if !credentials.is_empty() {
        out.push_str("Known credentials:\n");
        for cred in credentials {
            out.push_str(&format!(
                "  {}/{} (found on {})\n",
                cred.username, cred.password, cred.found_on
            ));
        }
    }

    let services = knowledge.services().collect::<Vec<_>>();
    if !services.is_empty() {
        out.push_str("Observed services:\n");
        for service in services {
            let state = if service.running {
                "running"
            } else {
                "stopped"
            };
            out.push_str(&format!(
                "  {}: {} {} ({state})\n",
                service.hostname, service.name, service.version
            ));
        }
    }

    if out.is_empty() {
        out.push_str("No intel gathered yet.\n");
    }
    CommandOutput::ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;
    use crate::world::{Device, Organization};

    #[test]
    fn whois_unknown_organization_fails() {
        let result = guest_shell().execute_line("whois Nobody Corp");
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn whois_known_organization_lists_its_infrastructure_and_records_knowledge() {
        let mut shell = guest_shell();
        shell.organizations.register(Organization::new(
            "Meridian Analytics",
            "A data firm.",
            vec!["meridian-web01".to_string(), "meridian-db01".to_string()],
        ));

        let result = shell.execute_line("whois Meridian Analytics");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("meridian-web01"));
        assert!(result.stdout.contains("meridian-db01"));

        assert!(shell.career.knowledge.knows_hostname("meridian-web01"));
        assert!(shell.career.knowledge.knows_hostname("meridian-db01"));
    }

    #[test]
    fn scan_unreachable_host_reports_it_without_touching_knowledge() {
        let mut shell = guest_shell();
        let result = shell.execute_line("scan nope");
        assert!(result.stdout.contains("unreachable"));
        assert!(!shell.career.knowledge.knows_hostname("nope"));
    }

    #[test]
    fn scan_without_nmap_reveals_only_reachability() {
        let mut shell = guest_shell();
        shell.network.register(Device::new("target01"));

        let result = shell.execute_line("scan target01");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("host is up"));
        assert!(!result.stdout.contains("nginx"));
        assert!(shell.career.knowledge.knows_hostname("target01"));
        assert_eq!(shell.career.knowledge.services().count(), 0);
    }

    #[test]
    fn scan_with_nmap_installed_reveals_services_and_versions() {
        let mut shell = guest_shell();
        shell.network.register(Device::new("target01"));
        shell
            .active_device_mut()
            .packages
            .install(true, "nmap")
            .unwrap();

        let result = shell.execute_line("scan target01");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("nginx"));
        assert!(result.stdout.contains("1.18.0"));
        assert!(
            shell
                .execute_line("intel")
                .stdout
                .contains("target01: nginx 1.18.0 (running)")
        );

        let target = shell.network.get_mut("target01").unwrap();
        target
            .services
            .stop(&mut target.processes, true, "nginx")
            .unwrap();
        assert!(
            shell
                .execute_line("intel")
                .stdout
                .contains("target01: nginx 1.18.0 (running)")
        );

        shell.execute_line("scan target01");
        assert!(
            shell
                .execute_line("intel")
                .stdout
                .contains("target01: nginx 1.18.0 (stopped)")
        );
        assert_eq!(shell.career.knowledge.services().count(), 3);

        shell.execute_line("connect target01 guest guest");
        assert!(
            shell
                .execute_line("intel")
                .stdout
                .contains("target01: nginx 1.18.0 (stopped)")
        );
    }

    #[test]
    fn intel_reports_no_discoveries_before_any_investigation() {
        let out = guest_shell().execute_line("intel").stdout;
        assert_eq!(out, "No intel gathered yet.\n");
    }

    #[test]
    fn intel_reviews_previously_discovered_hosts_and_credentials() {
        let mut shell = guest_shell();
        shell.organizations.register(Organization::new(
            "Meridian Analytics",
            "A data firm.",
            vec!["meridian-web01".to_string()],
        ));
        shell.execute_line("whois Meridian Analytics");
        shell
            .career
            .knowledge
            .record_credential("analyst", "hunter2", "meridian-web01");

        let out = shell.execute_line("intel").stdout;
        assert!(out.contains("meridian-web01"));
        assert!(out.contains("analyst/hunter2 (found on meridian-web01)"));
    }
}
