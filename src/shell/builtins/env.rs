use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn echo(_shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    CommandOutput::ok(format!("{}\n", args.join(" ")))
}

pub fn env(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let mut pairs: Vec<_> = shell.context.env.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let out = pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n");
    CommandOutput::ok(if out.is_empty() { out } else { out + "\n" })
}

pub fn export(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let mut err = String::new();
    for arg in args {
        match arg.split_once('=') {
            Some((k, v)) => {
                shell.context.env.insert(k.to_string(), v.to_string());
            }
            None => err.push_str(&format!("export: invalid assignment: {arg}\n")),
        }
    }
    if err.is_empty() {
        CommandOutput::empty_ok()
    } else {
        CommandOutput { stdout: String::new(), stderr: err, exit_code: 1 }
    }
}

pub fn which(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let Some(name) = args.first() else {
        return CommandOutput::error("which: missing argument\n");
    };
    if shell.has_builtin(name) {
        CommandOutput::ok(format!("/bin/{name}\n"))
    } else {
        let path = shell.context.env.get("PATH").cloned().unwrap_or_default();
        CommandOutput { stdout: String::new(), stderr: format!("which: no {name} in ({path})\n"), exit_code: 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;

    #[test]
    fn echo_joins_arguments_with_spaces() {
        assert_eq!(guest_shell().execute_line("echo hello world").stdout, "hello world\n");
    }

    #[test]
    fn env_var_expansion_flows_through_echo() {
        assert_eq!(guest_shell().execute_line("echo $HOME").stdout, "/home/guest\n");
    }

    #[test]
    fn export_makes_a_new_var_visible_to_later_commands() {
        let mut shell = guest_shell();
        shell.execute_line("export FOO=bar");
        assert_eq!(shell.execute_line("echo $FOO").stdout, "bar\n");
    }

    #[test]
    fn which_reports_a_path_for_known_builtins_and_errors_otherwise() {
        let mut shell = guest_shell();
        assert_eq!(shell.execute_line("which ls").stdout, "/bin/ls\n");
        assert_eq!(shell.execute_line("which nope").exit_code, 1);
    }
}
