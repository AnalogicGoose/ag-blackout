use crate::career::{Contract, Lead};

use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn contracts(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    match args.first().map(String::as_str) {
        None | Some("list") => CommandOutput::ok(list(shell)),
        Some("accept") => {
            let Some(id_arg) = args.get(1) else {
                return CommandOutput::error("contracts accept: missing id\n");
            };
            let Ok(id) = id_arg.parse::<u32>() else {
                return CommandOutput::error(format!("contracts accept: invalid id: {id_arg}\n"));
            };
            match shell.contracts.accept(id) {
                Ok(()) => CommandOutput::ok(format!("Accepted contract {id}")),
                Err(e) => CommandOutput::error(format!("contracts accept: {e}\n")),
            }
        }
        Some(other) => CommandOutput::error(format!(
            "contracts: unknown subcommand '{other}' (expected list/accept)\n"
        )),
    }
}

fn list(shell: &Shell) -> String {
    let mut out = String::new();

    let available: Vec<_> = shell.contracts.available().collect();
    if !available.is_empty() {
        out.push_str("Available:\n");
        for c in &available {
            out.push_str(&format!(
                "  [{}] {} — {} — ${}\n",
                c.id,
                c.title,
                briefing(c),
                c.reward
            ));
        }
    }

    let active: Vec<_> = shell.contracts.active().collect();
    if !active.is_empty() {
        out.push_str("Active:\n");
        for c in &active {
            out.push_str(&format!("  [{}] {} — {}\n", c.id, c.title, briefing(c)));
        }
    }

    let completed: Vec<_> = shell.contracts.completed().collect();
    if !completed.is_empty() {
        out.push_str("Completed:\n");
        for c in &completed {
            out.push_str(&format!("  [{}] {} — ${}\n", c.id, c.title, c.reward));
        }
    }

    if out.is_empty() {
        out.push_str("No contracts.\n");
    }
    out
}

/// What a contract's listing discloses about its target. A `Directed` lead
/// hands over the hostname and login outright; a `Guided` lead only names
/// the `Organization` and hint, never the resolved hostname — the player has
/// to discover it themselves. See docs/GAME_DESIGN.md's Slice 2 section.
fn briefing(c: &Contract) -> String {
    match &c.lead {
        Lead::Directed { username, password } => {
            format!("{} — login: {username}/{password}", c.target_hostname)
        }
        Lead::Guided { organization, hint } => format!("org: {organization} — hint: {hint}"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;
    use crate::career::{Contract, Objective};
    use crate::filesystem::VirtualPath;
    use crate::shell::Shell;

    fn with_one_posted_job(shell: &mut Shell) -> u32 {
        let path = VirtualPath::resolve(&VirtualPath::root(), "/home/finance/report.pdf").unwrap();
        shell.contracts.post(Contract::directed(
            "Get the report",
            "target01",
            path,
            3000,
            "guest",
            "guest",
            Objective::ObtainResource,
        ))
    }

    fn with_one_guided_job(shell: &mut Shell) -> u32 {
        let path =
            VirtualPath::resolve(&VirtualPath::root(), "/home/analyst/customers.csv").unwrap();
        shell.contracts.post(Contract::guided(
            "Obtain the customer database",
            "corp-db01",
            path,
            7500,
            "Meridian Analytics",
            "Operates in Managua, Nicaragua.",
            Objective::ObtainResource,
        ))
    }

    #[test]
    fn bare_contracts_lists_available_jobs() {
        let mut shell = guest_shell();
        with_one_posted_job(&mut shell);
        let out = shell.execute_line("contracts").stdout;
        assert!(out.contains("Available:"));
        assert!(out.contains("Get the report"));
        assert!(out.contains("login: guest/guest"));
    }

    #[test]
    fn guided_contracts_disclose_the_organization_and_hint_but_never_the_hostname() {
        let mut shell = guest_shell();
        with_one_guided_job(&mut shell);
        let out = shell.execute_line("contracts").stdout;
        assert!(out.contains("Obtain the customer database"));
        assert!(out.contains("org: Meridian Analytics"));
        assert!(out.contains("hint: Operates in Managua, Nicaragua."));
        assert!(!out.contains("corp-db01"));
        assert!(!out.contains("login:"));
    }

    #[test]
    fn no_contracts_says_so() {
        let out = guest_shell().execute_line("contracts").stdout;
        assert_eq!(out, "No contracts.\n");
    }

    #[test]
    fn accept_moves_a_contract_from_available_to_active() {
        let mut shell = guest_shell();
        let id = with_one_posted_job(&mut shell);
        let result = shell.execute_line(&format!("contracts accept {id}"));
        assert_eq!(result.exit_code, 0);
        let out = shell.execute_line("contracts").stdout;
        assert!(out.contains("Active:"));
        assert!(!out.contains("Available:"));
    }

    #[test]
    fn accept_unknown_id_fails() {
        let result = guest_shell().execute_line("contracts accept 999");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("no such contract"));
    }

    #[test]
    fn accept_twice_fails_the_second_time() {
        let mut shell = guest_shell();
        let id = with_one_posted_job(&mut shell);
        shell.execute_line(&format!("contracts accept {id}"));
        let result = shell.execute_line(&format!("contracts accept {id}"));
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not available"));
    }
}
