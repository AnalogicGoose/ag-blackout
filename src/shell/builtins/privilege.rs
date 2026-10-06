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
            shell.context = context;
            CommandOutput::empty_ok()
        }
        Err(error) => CommandOutput::error(format!("su: {error}\n")),
    }
}

pub fn sudo(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    if args.is_empty() {
        return CommandOutput::error("sudo: usage: sudo <command> [args...]\n");
    }

    let device = shell.active_device();
    if !device.sudoers.permits(&device.users, shell.context.uid) {
        let user = device.users.whoami(shell.context.uid).unwrap_or("unknown");
        return CommandOutput::error(format!("sudo: {user} is not in the sudoers file.\n"));
    }

    CommandOutput::error("sudo: interactive password prompt required\n")
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

        assert_eq!(shell.execute_sudo("sudo whoami", "admin123").stdout.trim(), "root");
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");

        assert_ne!(shell.execute_sudo("sudo whoami", "wrong").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");

        assert_ne!(shell.execute_sudo("sudo nonexistent", "admin123").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
    }

    #[test]
    fn sudo_denies_guest_and_session_switches() {
        let mut shell = guest_shell();

        assert_ne!(shell.execute_line("sudo whoami").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "guest");

        shell.execute_line("su admin admin123");
        assert_ne!(shell.execute_sudo("sudo disconnect", "admin123").exit_code, 0);
        assert_eq!(shell.execute_line("whoami").stdout.trim(), "admin");
    }
}
