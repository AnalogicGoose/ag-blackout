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
        Some(other) => CommandOutput::error(format!("contracts: unknown subcommand '{other}' (expected list/accept)\n")),
    }
}

fn list(shell: &Shell) -> String {
    let mut out = String::new();

    let available: Vec<_> = shell.contracts.available().collect();
    if !available.is_empty() {
        out.push_str("Available:\n");
        for c in &available {
            out.push_str(&format!("  [{}] {} — {} — ${}\n", c.id, c.title, c.target_hostname, c.reward));
        }
    }

    let active: Vec<_> = shell.contracts.active().collect();
    if !active.is_empty() {
        out.push_str("Active:\n");
        for c in &active {
            out.push_str(&format!("  [{}] {} — {}\n", c.id, c.title, c.target_hostname));
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

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;
    use crate::career::Contract;
    use crate::filesystem::VirtualPath;
    use crate::shell::Shell;

    fn with_one_posted_job(shell: &mut Shell) -> u32 {
        let path = VirtualPath::resolve(&VirtualPath::root(), "/home/finance/report.pdf").unwrap();
        shell.contracts.post(Contract::new("Get the report", "target01", path, 3000))
    }

    #[test]
    fn bare_contracts_lists_available_jobs() {
        let mut shell = guest_shell();
        with_one_posted_job(&mut shell);
        let out = shell.execute_line("contracts").stdout;
        assert!(out.contains("Available:"));
        assert!(out.contains("Get the report"));
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