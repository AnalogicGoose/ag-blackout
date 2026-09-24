use std::collections::HashMap;

use crate::filesystem::{VirtualFS, VirtualPath};
use crate::package::PackageManager;
use crate::system::{ExecutionContext, LogBook, ProcessTable, ServiceRegistry, Sudoers, UserDatabase};

use super::builtins::{self, CommandFn};
use super::output::{CommandOutput, LineResult};
use super::parser::{self, RedirectKind, Redirection};

/// A logged-in shell session: identity, the machine's filesystem, user
/// database, process/service state and package manager, and the command
/// table. `execute_line` is the one entry point — parse a raw line, run its
/// pipeline, return what would be printed.
pub struct Shell {
    pub context: ExecutionContext,
    pub filesystem: VirtualFS,
    pub users: UserDatabase,
    pub sudoers: Sudoers,
    pub processes: ProcessTable,
    pub services: ServiceRegistry,
    pub logs: LogBook,
    pub packages: PackageManager,
    builtins: HashMap<&'static str, CommandFn>,
}

impl Shell {
    pub fn new(
        filesystem: VirtualFS,
        users: UserDatabase,
        sudoers: Sudoers,
        context: ExecutionContext,
        processes: ProcessTable,
        services: ServiceRegistry,
        packages: PackageManager,
    ) -> Self {
        Shell {
            context,
            filesystem,
            users,
            sudoers,
            processes,
            services,
            packages,
            logs: LogBook::default(),
            builtins: builtins::table(),
        }
    }

    pub fn has_builtin(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
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
                    match self.filesystem.read_file(&self.context.fs_access(), &path) {
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
            let mut existing = self.filesystem.read_file(&access, &path).unwrap_or_default();
            existing.extend_from_slice(stdout.as_bytes());
            existing
        } else {
            stdout.as_bytes().to_vec()
        };
        if let Err(e) = self.filesystem.write_file(&access, &path, &bytes) {
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
}
