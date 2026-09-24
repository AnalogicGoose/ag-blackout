use crate::filesystem::VirtualPath;

use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn pwd(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    CommandOutput::ok(format!("{}\n", shell.context.cwd))
}

pub fn cd(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let target = match args.first() {
        Some(p) => p.clone(),
        None => shell.context.env.get("HOME").cloned().unwrap_or_else(|| "/".to_string()),
    };
    let path = match VirtualPath::resolve(&shell.context.cwd, &target) {
        Ok(p) => p,
        Err(e) => return CommandOutput::error(format!("cd: {target}: {e}\n")),
    };
    let access = shell.context.fs_access();
    match shell.active_device_mut().filesystem.is_dir(&access, &path) {
        Ok(true) => {
            shell.context.cwd = path;
            CommandOutput::empty_ok()
        }
        Ok(false) => CommandOutput::error(format!("cd: {target}: Not a directory\n")),
        Err(e) => CommandOutput::error(format!("cd: {target}: {e}\n")),
    }
}

pub fn ls(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let mut show_hidden = false;
    let mut long = false;
    let mut path_arg: Option<&str> = None;
    for a in args {
        match a.as_str() {
            "-a" => show_hidden = true,
            "-l" => long = true,
            "-la" | "-al" => {
                show_hidden = true;
                long = true;
            }
            other => path_arg = Some(other),
        }
    }

    let path = match VirtualPath::resolve(&shell.context.cwd, path_arg.unwrap_or(".")) {
        Ok(p) => p,
        Err(e) => return CommandOutput::error(format!("ls: {e}\n")),
    };
    let access = shell.context.fs_access();
    let mut entries = match shell.active_device_mut().filesystem.list_dir(&access, &path) {
        Ok(e) => e,
        Err(e) => {
            return CommandOutput::error(format!("ls: cannot access '{}': {e}\n", path_arg.unwrap_or(".")))
        }
    };
    entries.retain(|e| show_hidden || !e.is_hidden());
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    if entries.is_empty() {
        return CommandOutput::empty_ok();
    }

    let out = if long {
        let users = &shell.active_device().users;
        entries
            .iter()
            .map(|e| {
                let owner = users.user_by_uid(e.owner_uid).map(|u| u.username.clone()).unwrap_or_else(|| e.owner_uid.to_string());
                let group =
                    users.group_by_gid(e.group_gid).map(|g| g.name.clone()).unwrap_or_else(|| e.group_gid.to_string());
                format!("{} {owner:<8} {group:<8} {:>6} {}", e.permissions_string(), e.size, e.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>().join("  ") + "\n"
    };
    CommandOutput::ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;

    #[test]
    fn pwd_reports_home_for_a_fresh_login() {
        let mut shell = guest_shell();
        let result = shell.execute_line("pwd");
        assert_eq!(result.stdout, "/home/guest\n");
    }

    #[test]
    fn cd_changes_directory() {
        let mut shell = guest_shell();
        shell.execute_line("cd /tmp");
        assert_eq!(shell.execute_line("pwd").stdout, "/tmp\n");
    }

    #[test]
    fn cd_into_a_file_fails() {
        let mut shell = guest_shell();
        shell.execute_line("touch /tmp/file.txt");
        let result = shell.execute_line("cd /tmp/file.txt");
        assert!(result.stderr.contains("Not a directory"));
    }

    #[test]
    fn cd_without_permission_fails() {
        let mut shell = guest_shell();
        let result = shell.execute_line("cd /root");
        assert!(result.stderr.contains("permission denied"));
    }

    #[test]
    fn ls_hides_dotfiles_unless_dash_a() {
        let mut shell = guest_shell();
        shell.execute_line("touch /home/guest/.secret");
        shell.execute_line("touch /home/guest/visible");
        let plain = shell.execute_line("ls /home/guest");
        assert!(!plain.stdout.contains(".secret"));
        assert!(plain.stdout.contains("visible"));
        let all = shell.execute_line("ls -a /home/guest");
        assert!(all.stdout.contains(".secret"));
    }
}
