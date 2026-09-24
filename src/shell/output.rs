#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn ok(stdout: impl Into<String>) -> Self {
        CommandOutput { stdout: stdout.into(), stderr: String::new(), exit_code: 0 }
    }

    pub fn empty_ok() -> Self {
        Self::ok(String::new())
    }

    pub fn error(stderr: impl Into<String>) -> Self {
        CommandOutput { stdout: String::new(), stderr: stderr.into(), exit_code: 1 }
    }
}

/// What a whole input line (a pipeline, one or more `|`-joined stages)
/// produced: the last non-redirected stage's stdout, every stage's stderr in
/// order (stderr isn't piped between stages, same as a real shell), and the
/// last stage's exit code.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
