use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn whoami(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    match shell.active_device().users.whoami(shell.context.uid) {
        Some(name) => CommandOutput::ok(format!("{name}\n")),
        None => CommandOutput::error("whoami: cannot find username\n"),
    }
}

pub fn id(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let users = &shell.active_device().users;
    let uid = match args.first() {
        Some(name) => match users.user_by_name(name) {
            Some(u) => u.uid,
            None => return CommandOutput::error(format!("id: '{name}': no such user\n")),
        },
        None => shell.context.uid,
    };
    match shell.active_device().users.id_info(uid) {
        Some(info) => CommandOutput::ok(format!("{info}\n")),
        None => CommandOutput::error("id: cannot find user\n"),
    }
}

pub fn groups(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let users = &shell.active_device().users;
    let uid = match args.first() {
        Some(name) => match users.user_by_name(name) {
            Some(u) => u.uid,
            None => return CommandOutput::error(format!("groups: '{name}': no such user\n")),
        },
        None => shell.context.uid,
    };
    match shell.active_device().users.groups_of(uid) {
        Some(gs) => {
            let names = gs
                .iter()
                .map(|g| g.name.clone())
                .collect::<Vec<_>>()
                .join(" ");
            CommandOutput::ok(format!("{names}\n"))
        }
        None => CommandOutput::error("groups: cannot find user\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::{admin_shell, guest_shell};

    #[test]
    fn whoami_reports_the_logged_in_username() {
        assert_eq!(guest_shell().execute_line("whoami").stdout, "guest\n");
    }

    #[test]
    fn id_reports_full_identity_string() {
        let result = admin_shell().execute_line("id");
        assert_eq!(
            result.stdout,
            "uid=1000(admin) gid=1000(admin) groups=1000(admin),27(sudo)\n"
        );
    }

    #[test]
    fn groups_lists_group_names() {
        let result = admin_shell().execute_line("groups");
        assert_eq!(result.stdout, "admin sudo\n");
    }

    #[test]
    fn id_of_unknown_user_fails() {
        let result = guest_shell().execute_line("id nobody");
        assert_eq!(result.exit_code, 1);
    }
}
