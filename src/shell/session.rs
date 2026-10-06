use std::collections::HashMap;

use crate::career::{Career, ContractBoard, Economy};
use crate::filesystem::VirtualPath;
use crate::system::ExecutionContext;
use crate::world::{Device, Network, OrganizationRegistry};

use super::builtins::{self, CommandFn};
use super::output::{CommandOutput, LineResult};
use super::parser::{self, RedirectKind, Redirection};

/// A logged-in shell session. Owns the player's world (`Network`,
/// `OrganizationRegistry`) and career state (`ContractBoard`, `Economy`,
/// `Career`) — those persist across `connect`/`disconnect` — plus which
/// device the session is currently attached to and the identity it's
/// authenticated as there. `execute_line` is the one entry point — parse a
/// raw line, run its pipeline against whichever device is active, return
/// what would be printed. `Career` (holding `Knowledge`) is a distinct field
/// rather than something `Shell` computes or owns outright — see
/// docs/GAME_DESIGN.md's Slice 2 section on why `Knowledge` doesn't belong
/// directly on session state.
pub struct Shell {
    pub network: Network,
    pub organizations: OrganizationRegistry,
    pub economy: Economy,
    pub contracts: ContractBoard,
    pub career: Career,
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
            organizations: OrganizationRegistry::new(),
            economy: Economy::new(),
            contracts: ContractBoard::new(),
            career: Career::new(),
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

    /// If the active device has a `CredentialLead` at exactly `path`, records
    /// it into `career.knowledge` — called after a successful read (`cat`)
    /// of that path. Reading is otherwise a plain filesystem operation; this
    /// is the one place a read has a side effect beyond its own output. See
    /// docs/GAME_DESIGN.md's Slice 2 section.
    pub fn note_credential_leads_at(&mut self, path: &VirtualPath) {
        let host = self.active_hostname.clone();
        let Some(lead) = self.active_device().credential_leads.iter().find(|lead| &lead.path == path) else { return };
        let (username, password) = (lead.username.clone(), lead.password.clone());
        self.career.knowledge.record_credential(username, password, host);
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

    /// The local session's cwd — where a relative `local_path` argument to
    /// `download` resolves against, independent of wherever `context.cwd`
    /// currently points on a connected remote device.
    pub fn local_cwd(&self) -> &VirtualPath {
        &self.local_context.cwd
    }

    /// `download`'s actual logic: reads `remote_path` off the currently
    /// active (must be remote) device and writes its bytes into the local
    /// device at `local_path`. If `local_path` names an existing local
    /// directory, the file is placed inside it under the remote file's
    /// basename (matching `cp`/`scp`'s "directory destination" behavior,
    /// rather than overwriting the directory's own name with the file); with
    /// no `local_path` at all, it lands under the local identity's home
    /// directory the same way. This — not `cat` — is what "obtaining a
    /// resource" means for a contract now (see docs/GAME_DESIGN.md):
    /// proof-of-access alone no longer pays out, the file has to actually
    /// make it home.
    pub fn download(&mut self, remote_path: &VirtualPath, local_path: Option<&VirtualPath>) -> Result<VirtualPath, String> {
        if !self.is_connected_remotely() {
            return Err("not connected to a remote host".to_string());
        }

        let remote_access = self.context.fs_access();
        let contents = self
            .active_device_mut()
            .filesystem
            .read_file(&remote_access, remote_path)
            .map_err(|e| format!("{remote_path}: {e}"))?;

        let local_access = self.local_context.fs_access();
        let local_path = match local_path {
            Some(p) => {
                let is_dir = self.network.get(&self.local_hostname).expect("local_hostname always names a registered device")
                    .filesystem
                    .is_dir(&local_access, p)
                    .unwrap_or(false);
                if is_dir {
                    let filename = remote_path
                        .file_name()
                        .ok_or_else(|| format!("{remote_path}: no filename to save"))?;
                    p.join(filename)
                } else {
                    p.clone()
                }
            }
            None => {
                let filename = remote_path
                    .file_name()
                    .ok_or_else(|| format!("{remote_path}: no filename to save"))?;
                let home = self.local_context.env.get("HOME").expect("every context has HOME").clone();
                VirtualPath::resolve(&VirtualPath::root(), &home)
                    .expect("HOME is always a valid absolute path")
                    .join(filename)
            }
        };
        let local_hostname = self.local_hostname.clone();
        self.network
            .get_mut(&local_hostname)
            .expect("local_hostname always names a registered device")
            .filesystem
            .write_file(&local_access, &local_path, &contents)
            .map_err(|e| format!("{local_path}: {e}"))?;

        let remote_host = self.active_hostname.clone();
        for contract in self.contracts.record_download(&remote_host, remote_path) {
            self.economy.deposit(contract.reward);
        }

        Ok(local_path)
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
        match self.active_device_mut().filesystem.write_file(&access, &path, &bytes) {
            Ok(()) => {
                // A `ModifyResource` contract is satisfied by overwriting its
                // resource with the required bytes — see docs/GAME_DESIGN.md.
                let host = self.active_hostname.clone();
                for contract in self.contracts.record_modify(&host, &path, &bytes) {
                    self.economy.deposit(contract.reward);
                }
            }
            Err(e) => stderr_acc.push_str(&format!("ag-shell: {}: {e}\n", redir.target)),
        }
    }

    fn resolve(&self, raw: &str) -> VirtualPath {
        VirtualPath::resolve(&self.context.cwd, raw).unwrap_or_else(|_| self.context.cwd.clone())
    }

    fn run_command(&mut self, argv: &[String], stdin: Option<&str>) -> CommandOutput {
        let Some(name) = argv.first() else { return CommandOutput::empty_ok() };
        let Some(f) = self.builtins.get(name.as_str()).copied() else {
            return CommandOutput::error(format!("ag-shell: {name}: command not found\n"));
        };
        if argv[1..].iter().any(|a| a == "-h" || a == "--help") {
            return CommandOutput::ok(builtins::help_text(name));
        }
        f(self, &argv[1..], stdin)
    }

    pub(crate) fn run_as(
        &mut self,
        context: ExecutionContext,
        argv: &[String],
        stdin: Option<&str>,
    ) -> CommandOutput {
        let previous = std::mem::replace(&mut self.context, context);
        let output = self.run_command(argv, stdin);
        self.context = previous;
        output
    }

    pub fn execute_sudo(&mut self, line: &str, password: &str) -> LineResult {
        let pipeline = match parser::parse(line, &self.context.env) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                return LineResult {
                    stdout: String::new(),
                    stderr: format!("sudo: {error}\n"),
                    exit_code: 2,
                }
            }
        };

        if pipeline.stages.len() != 1 || !pipeline.stages[0].redirections.is_empty() {
            return LineResult {
                stdout: String::new(),
                stderr: "sudo: pipelines and redirections are not supported\n".into(),
                exit_code: 2,
            };
        }

        let argv = &pipeline.stages[0].argv;
        if argv.first().map(String::as_str) != Some("sudo") || argv.len() < 2 {
            return LineResult {
                stdout: String::new(),
                stderr: "sudo: usage: sudo <command> [args...]\n".into(),
                exit_code: 2,
            };
        }

        let command = &argv[1..];
        if matches!(
            command[0].as_str(),
            "connect" | "disconnect" | "su" | "sudo"
        ) {
            return LineResult {
                stdout: String::new(),
                stderr: format!("sudo: cannot run {}\n", command[0]),
                exit_code: 1,
            };
        }

        let context = match crate::system::sudo(
            &self.active_device().users,
            &self.active_device().sudoers,
            &self.context,
            password,
            None,
        ) {
            Ok(context) => context,
            Err(error) => {
                return LineResult {
                    stdout: String::new(),
                    stderr: format!("sudo: {error}\n"),
                    exit_code: 1,
                };
            }
        };

        let output = self.run_as(context, command, None);
        LineResult {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
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
    fn dash_h_and_dash_dash_help_print_usage_instead_of_running_the_command() {
        let mut shell = guest_shell();
        for flag in ["-h", "--help"] {
            let result = shell.execute_line(&format!("ls {flag}"));
            assert_eq!(result.exit_code, 0);
            assert!(result.stdout.contains("Usage: ls"));
        }
    }

    #[test]
    fn help_flag_on_an_unknown_command_still_reports_not_found() {
        let result = guest_shell().execute_line("frobnicate --help");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("command not found"));
    }

    #[test]
    fn help_flag_does_not_run_the_command_it_shortcuts() {
        let mut shell = guest_shell();
        // mkdir --help must not actually create anything.
        shell.execute_line("mkdir --help /home/guest/should-not-exist");
        assert_eq!(shell.execute_line("cd /home/guest/should-not-exist").exit_code, 1);
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

    #[test]
    fn download_requires_being_connected_remotely() {
        let mut shell = guest_shell();
        let path = crate::filesystem::VirtualPath::resolve(&crate::filesystem::VirtualPath::root(), "/home/guest/x.txt").unwrap();
        assert!(shell.download(&path, None).is_err());
    }

    #[test]
    fn download_copies_the_remote_file_home_and_resolves_a_matching_contract() {
        use crate::career::Contract;
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = crate::world::Device::new("target01");
        let remote_path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target.filesystem.write_file(&FsAccess::root(), &remote_path, b"confidential").unwrap();
        shell.network.register(target);

        let id = shell.contracts.post(Contract::directed("Get the report", "target01", remote_path.clone(), 3000, "guest", "guest", crate::career::Objective::ObtainResource));
        shell.contracts.accept(id).unwrap();
        shell.connect("target01", "guest", "guest").unwrap();

        let local_path = shell.download(&remote_path, None).unwrap();
        assert_eq!(local_path.to_string(), "/home/guest/report.pdf");
        assert_eq!(shell.economy.balance(), 3000);
        assert_eq!(shell.contracts.completed().count(), 1);

        shell.disconnect().unwrap();
        assert_eq!(shell.execute_line("cat /home/guest/report.pdf").stdout, "confidential");
    }

    #[test]
    fn download_honors_an_explicit_local_path() {
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = crate::world::Device::new("target01");
        let remote_path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target.filesystem.write_file(&FsAccess::root(), &remote_path, b"confidential").unwrap();
        shell.network.register(target);
        shell.connect("target01", "guest", "guest").unwrap();

        let local_path = VirtualPath::resolve(&VirtualPath::root(), "/tmp/loot.pdf").unwrap();
        let saved = shell.download(&remote_path, Some(&local_path)).unwrap();
        assert_eq!(saved, local_path);

        shell.disconnect().unwrap();
        assert_eq!(shell.execute_line("cat /tmp/loot.pdf").stdout, "confidential");
    }

    #[test]
    fn download_into_an_existing_local_directory_keeps_the_remote_basename() {
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = crate::world::Device::new("target01");
        let remote_path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target.filesystem.write_file(&FsAccess::root(), &remote_path, b"confidential").unwrap();
        shell.network.register(target);
        shell.connect("target01", "guest", "guest").unwrap();

        // /tmp already exists as a directory in the default tree.
        let local_dir = VirtualPath::resolve(&VirtualPath::root(), "/tmp").unwrap();
        let saved = shell.download(&remote_path, Some(&local_dir)).unwrap();
        assert_eq!(saved.to_string(), "/tmp/report.pdf");

        shell.disconnect().unwrap();
        assert_eq!(shell.execute_line("cat /tmp/report.pdf").stdout, "confidential");
    }
}
