use std::collections::HashMap;

use crate::career::{ContractBoard, Economy};
use crate::filesystem::VirtualPath;
use crate::system::ExecutionContext;
use crate::world::{Device, Network};

use super::builtins::{self, CommandFn};
use super::output::{CommandOutput, LineResult};
use super::parser::{self, RedirectKind, Redirection};

/// A logged-in shell session. Owns the player's world (`Network`) and career
/// state (`ContractBoard`, `Economy`) — those persist across `connect`/
/// `disconnect` — plus which device the session is currently attached to and
/// the identity it's authenticated as there. `execute_line` is the one entry
/// point — parse a raw line, run its pipeline against whichever device is
/// active, return what would be printed.
pub struct Shell {
    pub network: Network,
    pub economy: Economy,
    pub contracts: ContractBoard,
    pub context: ExecutionContext,
    local_hostname: String,
    active_hostname: String,
    local_context: ExecutionContext,
    builtins: HashMap<&'static str, CommandFn>,
}

impl Shell {
    /// Boots a session logged into `initial_uid` on whichever device in
    /// `network` is named `local_hostname` — that's "home," what `disconnect`
    /// returns to.
    pub fn new(network: Network, local_hostname: impl Into<String>, initial_uid: u32) -> Self {
        let local_hostname = local_hostname.into();
        let local_context = network
            .get(&local_hostname)
            .and_then(|device| device.users.execution_context_for(initial_uid))
            .expect("local_hostname must be registered in network and initial_uid must exist on it");
        Shell {
            network,
            economy: Economy::new(),
            contracts: ContractBoard::new(),
            context: local_context.clone(),
            active_hostname: local_hostname.clone(),
            local_hostname,
            local_context,
            builtins: builtins::table(),
        }
    }

    pub fn has_builtin(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
    }

    /// The device the session is currently attached to — local by default,
    /// whatever `connect` last targeted otherwise.
    pub fn active_device(&self) -> &Device {
        self.network.get(&self.active_hostname).expect("active_hostname always names a registered device")
    }

    pub fn active_device_mut(&mut self) -> &mut Device {
        self.network.get_mut(&self.active_hostname).expect("active_hostname always names a registered device")
    }

    pub fn is_connected_remotely(&self) -> bool {
        self.active_hostname != self.local_hostname
    }

    pub fn active_hostname(&self) -> &str {
        &self.active_hostname
    }

    /// `connect`'s actual logic, callable directly by the builtin. Looks the
    /// host up on the network, authenticates against *its* user database,
    /// and — on success — switches the session's active device and identity.
    pub fn connect(&mut self, hostname: &str, username: &str, password: &str) -> Result<(), String> {
        let device = self.network.get(hostname).ok_or_else(|| format!("{hostname}: no route to host"))?;
        let uid = device.users.authenticate(username, password).ok_or_else(|| format!("{hostname}: authentication failed"))?;
        let ctx = device.users.execution_context_for(uid).expect("authenticate just confirmed this uid exists");
        self.active_hostname = hostname.to_string();
        self.context = ctx;
        Ok(())
    }

    /// `disconnect`'s actual logic. Returns to the local device/identity.
    pub fn disconnect(&mut self) -> Result<(), String> {
        if !self.is_connected_remotely() {
            return Err("not connected to a remote host".to_string());
        }
        self.active_hostname = self.local_hostname.clone();
        self.context = self.local_context.clone();
        Ok(())
    }

    pub fn execute_line(&mut self, input: &str) -> LineResult {
        let pipeline = match parser::parse(input, &self.context.env) {
            Ok(p) => p,
            Err(e) => return LineResult { stdout: String::new(), stderr: format!("ag-shell: {e}\n"), exit_code: 2 },
        };
        if pipeline.stages.is_empty() {
            return LineResult::default();
        }

        let mut stdin: Option<String> = None;
        let mut stderr_acc = String::new();
        let mut last = CommandOutput::empty_ok();

        for stage in &pipeline.stages {
            let stage_stdin = match stage.redirections.iter().find(|r| r.kind == RedirectKind::In) {
                Some(redir) => {
                    let path = self.resolve(&redir.target);
                    let access = self.context.fs_access();
                    match self.active_device_mut().filesystem.read_file(&access, &path) {
                        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                        Err(e) => {
                            stderr_acc.push_str(&format!("ag-shell: {}: {e}\n", redir.target));
                            None
                        }
                    }
                }
                None => stdin.take(),
            };

            let output = self.run_command(&stage.argv, stage_stdin.as_deref());
            stderr_acc.push_str(&output.stderr);

            let out_redirs: Vec<&Redirection> =
                stage.redirections.iter().filter(|r| r.kind != RedirectKind::In).collect();

            if out_redirs.is_empty() {
                stdin = Some(output.stdout.clone());
                last = output;
            } else {
                for redir in &out_redirs {
                    self.write_redirect(redir, &output.stdout, &mut stderr_acc);
                }
                last = CommandOutput { stdout: String::new(), stderr: String::new(), exit_code: output.exit_code };
                stdin = Some(String::new());
            }
        }

        LineResult { stdout: last.stdout, stderr: stderr_acc, exit_code: last.exit_code }
    }

    fn write_redirect(&mut self, redir: &Redirection, stdout: &str, stderr_acc: &mut String) {
        let path = self.resolve(&redir.target);
        let access = self.context.fs_access();
        let bytes = if redir.kind == RedirectKind::Append {
            let mut existing = self.active_device_mut().filesystem.read_file(&access, &path).unwrap_or_default();
            existing.extend_from_slice(stdout.as_bytes());
            existing
        } else {
            stdout.as_bytes().to_vec()
        };
        if let Err(e) = self.active_device_mut().filesystem.write_file(&access, &path, &bytes) {
            stderr_acc.push_str(&format!("ag-shell: {}: {e}\n", redir.target));
        }
    }

    fn resolve(&self, raw: &str) -> VirtualPath {
        VirtualPath::resolve(&self.context.cwd, raw).unwrap_or_else(|_| self.context.cwd.clone())
    }

    fn run_command(&mut self, argv: &[String], stdin: Option<&str>) -> CommandOutput {
        let Some(name) = argv.first() else { return CommandOutput::empty_ok() };
        match self.builtins.get(name.as_str()).copied() {
            Some(f) => f(self, &argv[1..], stdin),
            None => CommandOutput::error(format!("ag-shell: {name}: command not found\n")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::guest_shell;

    #[test]
    fn unknown_command_reports_not_found() {
        let result = guest_shell().execute_line("frobnicate");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("command not found"));
    }

    #[test]
    fn pipeline_feeds_stdout_into_next_stage() {
        let mut shell = guest_shell();
        shell.execute_line("echo hello > /home/guest/note.txt");
        let result = shell.execute_line("cat /home/guest/note.txt | cat");
        assert_eq!(result.stdout, "hello\n");
    }

    #[test]
    fn output_redirection_writes_to_a_file_instead_of_stdout() {
        let mut shell = guest_shell();
        let result = shell.execute_line("echo hi > /home/guest/out.txt");
        assert_eq!(result.stdout, "");
        assert_eq!(shell.execute_line("cat /home/guest/out.txt").stdout, "hi\n");
    }

    #[test]
    fn append_redirection_adds_to_existing_content() {
        let mut shell = guest_shell();
        shell.execute_line("echo one > /home/guest/log.txt");
        shell.execute_line("echo two >> /home/guest/log.txt");
        assert_eq!(shell.execute_line("cat /home/guest/log.txt").stdout, "one\ntwo\n");
    }

    #[test]
    fn input_redirection_reads_stdin_from_a_file() {
        let mut shell = guest_shell();
        shell.execute_line("echo piped > /home/guest/in.txt");
        let result = shell.execute_line("cat < /home/guest/in.txt");
        assert_eq!(result.stdout, "piped\n");
    }

    #[test]
    fn parse_errors_surface_as_a_line_result() {
        let result = guest_shell().execute_line("ls |");
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("ag-shell:"));
    }

    #[test]
    fn blank_line_is_a_silent_no_op() {
        let result = guest_shell().execute_line("   ");
        assert_eq!(result, super::super::output::LineResult::default());
    }

    #[test]
    fn connect_switches_active_device_and_identity() {
        let mut shell = guest_shell();
        shell.network.register(crate::world::Device::new("target01"));
        shell.connect("target01", "guest", "guest").unwrap();
        assert!(shell.is_connected_remotely());
        assert_eq!(shell.execute_line("whoami").stdout, "guest\n");
    }

    #[test]
    fn connect_with_wrong_password_fails() {
        let mut shell = guest_shell();
        shell.network.register(crate::world::Device::new("target01"));
        assert!(shell.connect("target01", "guest", "wrong").is_err());
        assert!(!shell.is_connected_remotely());
    }

    #[test]
    fn disconnect_restores_local_identity_and_cwd() {
        let mut shell = guest_shell();
        shell.network.register(crate::world::Device::new("target01"));
        shell.connect("target01", "guest", "guest").unwrap();
        shell.execute_line("cd /tmp");
        shell.disconnect().unwrap();
        assert!(!shell.is_connected_remotely());
        assert_eq!(shell.execute_line("pwd").stdout, "/home/guest\n");
    }

    #[test]
    fn disconnect_without_a_connection_fails() {
        let mut shell = guest_shell();
        assert!(shell.disconnect().is_err());
    }
}
