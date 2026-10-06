use super::super::output::CommandOutput;
use super::super::session::Shell;

/// The TUI owns scrollback, so this builtin only validates the command.
pub fn clear(_shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    if args.is_empty() {
        CommandOutput::empty_ok()
    } else {
        CommandOutput::error("clear: usage: clear\n")
    }
}

/// A lesson swaps the active Shell, which only GameSession can do.
pub fn tutorial(_shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    CommandOutput::error("tutorial: available in the interactive game terminal\n")
}
