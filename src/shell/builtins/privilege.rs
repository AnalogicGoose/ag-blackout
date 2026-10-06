use crate::system::su as switch_user;

use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn su(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let [username, password] = args else {
        return CommandOutput::error("su: usage: su <user> <password>\n");
    };

    match switch_user(
        &shell.active_device().users,
        &shell.context,
        username,
        Some(password),
        false,
    ) {
        Ok(context) => {
            shell.clear_sudo_cache();
            shell.context = context;
            CommandOutput::empty_ok()
        }
        Err(error) => CommandOutput::error(format!("su: {error}\n")),
    }
}

pub fn sudo(shell: &mut Shell, args: &[String], stdin: Option<&str>) -> CommandOutput {
    shell.run_sudo_command(args, None, stdin)
}

#[cfg(test)]
mod tests {
    use crate::shell::test_support::guest_shell;

    #[test]
    fn su_switches_identity_only_with_the_target_password() {
        let mut shell = guest_shell();

        assert_ne!(shell.execute_line("su admin wrong").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "guest");

        assert_eq!(shell.execute_line("su admin admin123").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
    }

    #[test]
    fn sudo_runs_one_command_and_restores_the_caller() {
        let mut shell = guest_shell();
        shell.execute_line("su admin admin123");

        assert_ne!(shell.execute_sudo("sudo whoami", "wrong").exit_code, 0);
        assert_eq!(
            shell.execute_sudo("sudo whoami", "admin123").stdout.trim(),
            "root"
        );
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");

        assert_eq!(shell.execute_line("sudo whoami").stdout.trim(), "root");
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");

        assert_ne!(
            shell.execute_sudo("sudo nonexistent", "admin123").exit_code,
            0
        );
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
    }

    #[test]
    fn sudo_denies_guest_and_session_switches() {
        let mut shell = guest_shell();

        assert_ne!(shell.execute_line("sudo whoami").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "guest");

        shell.execute_line("su admin admin123");
        assert_ne!(
            shell.execute_sudo("sudo disconnect", "admin123").exit_code,
            0
        );
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
    }
}
