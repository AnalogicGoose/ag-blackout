use crate::system::ServiceState;

use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn ps(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let mut out = String::from("PID USER     COMMAND\n");
    let device = shell.active_device();
    for p in device.processes.list() {
        let user = device.users.user_by_uid(p.uid).map(|u| u.username.clone()).unwrap_or_else(|| p.uid.to_string());
        out.push_str(&format!("{:<4}{user:<9}{}\n", p.pid, p.command));
    }
    CommandOutput::ok(out)
}

pub fn kill(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let Some(pid_arg) = args.first() else {
        return CommandOutput::error("kill: missing pid\n");
    };
    let Ok(pid) = pid_arg.parse::<u32>() else {
        return CommandOutput::error(format!("kill: invalid pid: {pid_arg}\n"));
    };
    let uid = shell.context.uid;
    let is_root = shell.context.is_root();
    let device = shell.active_device_mut();
    match device.processes.kill(uid, is_root, pid) {
        Ok(process) => {
            device.logs.record("kill", format!("{} (pid {}) terminated by uid {uid}", process.command, process.pid));
            CommandOutput::empty_ok()
        }
        Err(e) => CommandOutput::error(format!("kill: ({pid}): {e}\n")),
    }
}

pub fn service(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let Some(name) = args.first() else {
        let mut out = String::new();
        for s in shell.active_device().services.list() {
            let state = if s.state == ServiceState::Running { "running" } else { "stopped" };
            out.push_str(&format!("{:<10}{state}\n", s.name));
        }
        return CommandOutput::ok(out);
    };
    let action = args.get(1).map(String::as_str).unwrap_or("status");
    let uid = shell.context.uid;
    let is_root = shell.context.is_root();

    match action {
        "status" => match shell.active_device().services.get(name) {
            Some(s) => {
                let state = if s.state == ServiceState::Running { "running" } else { "stopped" };
                CommandOutput::ok(format!("{} is {state}\n", s.name))
            }
            None => CommandOutput::error(format!("service: unknown service: {name}\n")),
        },
        "start" => {
            let device = shell.active_device_mut();
            match device.services.start(&mut device.processes, is_root, name) {
                Ok(()) => {
                    device.logs.record("service", format!("{name} started by uid {uid}"));
                    CommandOutput::empty_ok()
                }
                Err(e) => CommandOutput::error(format!("service: {name}: {e}\n")),
            }
        }
        "stop" => {
            let device = shell.active_device_mut();
            match device.services.stop(&mut device.processes, is_root, name) {
                Ok(()) => {
                    device.logs.record("service", format!("{name} stopped by uid {uid}"));
                    CommandOutput::empty_ok()
                }
                Err(e) => CommandOutput::error(format!("service: {name}: {e}\n")),
            }
        }
        other => CommandOutput::error(format!("service: unknown action '{other}' (expected status/start/stop)\n")),
    }
}

pub fn logs(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let out =
        shell.active_device().logs.entries().iter().map(|e| format!("{}: {}", e.source, e.message)).collect::<Vec<_>>().join("\n");
    CommandOutput::ok(if out.is_empty() { out } else { out + "\n" })
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::{admin_shell, guest_shell};

    #[test]
    fn ps_lists_default_processes() {
        let out = guest_shell().execute_line("ps").stdout;
        assert!(out.contains("ag-init"));
        assert!(out.contains("sshd"));
    }

    #[test]
    fn guest_cannot_kill_root_owned_process() {
        let mut shell = guest_shell();
        let pid = shell.active_device().processes.list().iter().find(|p| p.command == "sshd").unwrap().pid;
        let result = shell.execute_line(&format!("kill {pid}"));
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn root_can_kill_any_process() {
        let mut shell = admin_shell();
        shell.context.uid = 0; // pretend we escalated for this check
        let pid = shell.active_device().processes.list().iter().find(|p| p.command == "sshd").unwrap().pid;
        let result = shell.execute_line(&format!("kill {pid}"));
        assert_eq!(result.exit_code, 0);
        assert!(shell.execute_line("logs").stdout.contains("terminated"));
    }

    #[test]
    fn cannot_kill_init() {
        let mut shell = guest_shell();
        shell.context.uid = 0;
        let result = shell.execute_line("kill 1");
        assert!(result.stderr.contains("init"));
    }

    #[test]
    fn service_status_reports_running() {
        let result = guest_shell().execute_line("service sshd status");
        assert_eq!(result.stdout, "sshd is running\n");
    }

    #[test]
    fn service_stop_requires_root() {
        let mut shell = guest_shell();
        let result = shell.execute_line("service sshd stop");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("root"));
    }

    #[test]
    fn service_stop_and_start_as_root_round_trip() {
        let mut shell = guest_shell();
        shell.context.uid = 0;
        assert_eq!(shell.execute_line("service sshd stop").exit_code, 0);
        assert_eq!(shell.execute_line("service sshd status").stdout, "sshd is stopped\n");
        assert_eq!(shell.execute_line("service sshd start").exit_code, 0);
        assert_eq!(shell.execute_line("service sshd status").stdout, "sshd is running\n");
        assert!(shell.execute_line("logs").stdout.contains("stopped by uid 0"));
    }

    #[test]
    fn service_with_no_args_lists_all() {
        let out = guest_shell().execute_line("service").stdout;
        assert!(out.contains("sshd"));
        assert!(out.contains("nginx"));
    }
}
