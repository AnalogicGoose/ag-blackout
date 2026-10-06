mod agpkg;
mod contracts;
mod env;
mod files;
mod identity;
mod navigation;
mod network;
mod privilege;
mod process;
mod recon;

use std::collections::HashMap;

use super::output::CommandOutput;
use super::session::Shell;

pub type CommandFn = fn(&mut Shell, &[String], Option<&str>) -> CommandOutput;

pub(super) fn split_flags(args: &[String]) -> (Vec<&str>, Vec<String>) {
    let mut flags = Vec::new();
    let mut rest = Vec::new();
    for a in args {
        if a.starts_with('-') && a.len() > 1 {
            flags.push(a.as_str());
        } else {
            rest.push(a.clone());
        }
    }
    (flags, rest)
}

pub fn table() -> HashMap<&'static str, CommandFn> {
    let mut m: HashMap<&'static str, CommandFn> = HashMap::new();
    m.insert("pwd", navigation::pwd);
    m.insert("cd", navigation::cd);
    m.insert("ls", navigation::ls);
    m.insert("cat", files::cat);
    m.insert("touch", files::touch);
    m.insert("mkdir", files::mkdir);
    m.insert("rm", files::rm);
    m.insert("cp", files::cp);
    m.insert("mv", files::mv);
    m.insert("whoami", identity::whoami);
    m.insert("id", identity::id);
    m.insert("groups", identity::groups);
    m.insert("echo", env::echo);
    m.insert("env", env::env);
    m.insert("export", env::export);
    m.insert("which", env::which);
    m.insert("ps", process::ps);
    m.insert("kill", process::kill);
    m.insert("service", process::service);
    m.insert("logs", process::logs);
    m.insert("agpkg", agpkg::agpkg);
    m.insert("connect", network::connect);
    m.insert("disconnect", network::disconnect);
    m.insert("download", network::download);
    m.insert("contracts", contracts::contracts);
    m.insert("whois", recon::whois);
    m.insert("scan", recon::scan);
    m.insert("intel", recon::intel);
    m.insert("help", help);
    m.insert("su", privilege::su);
    m.insert("sudo", privilege::sudo);
    m
}

/// Lists every registered command with the one-line description from its
/// own `help_text` entry (the `<name> - <description>` first line) — kept in
/// sync with `help_text` automatically instead of duplicating descriptions.
pub fn help(_shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let mut names: Vec<&str> = table().keys().copied().collect();
    names.sort();

    let mut out = String::from("Available commands (run '<command> --help' for details):\n");
    for name in names {
        let text = help_text(name);
        let description = text
            .lines()
            .next()
            .and_then(|line| line.split_once(" - "))
            .map(|(_, d)| d)
            .unwrap_or("");
        out.push_str(&format!("  {name:<11}{description}\n"));
    }
    CommandOutput::ok(out)
}

/// One usage string per builtin, shown by `<command> -h`/`--help` (see
/// `Shell::run_command`'s interception). Kept as a flat lookup here, next to
/// `table()`, rather than baked into each builtin function, so every command
/// gets the same flag without touching its own logic.
pub(super) fn help_text(name: &str) -> String {
    let text = match name {
        "pwd" => "pwd - print the current working directory\nUsage: pwd\n",
        "cd" => {
            "cd - change the current directory\nUsage: cd [path]\n\nWith no path, changes to $HOME.\n"
        }
        "ls" => {
            "ls - list directory contents\nUsage: ls [-a] [-l] [-la|-al] [path]\n\n  -a       show hidden (dot-prefixed) entries\n  -l       long listing (permissions, owner, group, size, name)\n  -la/-al  combine -a and -l\n\nWith no path, lists the current directory.\n"
        }
        "cat" => {
            "cat - print file contents\nUsage: cat [path ...]\n\nWith no paths and input piped in, prints stdin instead. A plain read — never resolves a contract by itself; see 'download'.\n"
        }
        "touch" => {
            "touch - create an empty file, or update its timestamp if it already exists\nUsage: touch <path> [path ...]\n"
        }
        "mkdir" => {
            "mkdir - create a directory\nUsage: mkdir [-p] <path> [path ...]\n\n  -p  create intermediate directories as needed, and don't error if the path already exists\n"
        }
        "rm" => {
            "rm - remove a file or directory\nUsage: rm [-r|-R] <path> [path ...]\n\n  -r, -R  remove directories and their contents recursively\n"
        }
        "cp" => "cp - copy a file\nUsage: cp <source> <destination>\n",
        "mv" => "mv - rename or move a file\nUsage: mv <source> <destination>\n",
        "whoami" => "whoami - print the current username\nUsage: whoami\n",
        "id" => {
            "id - print user and group identity\nUsage: id [username]\n\nWith no username, reports the current identity.\n"
        }
        "groups" => {
            "groups - list a user's group memberships\nUsage: groups [username]\n\nWith no username, reports the current identity's groups.\n"
        }
        "echo" => {
            "echo - print arguments\nUsage: echo [text ...]\n\nArguments are joined with a single space.\n"
        }
        "env" => "env - list environment variables\nUsage: env\n",
        "export" => {
            "export - set an environment variable\nUsage: export NAME=value [NAME=value ...]\n"
        }
        "which" => {
            "which - report what a command resolves to\nUsage: which <command>\n\nOnly aware of shell builtins, not AGPKG-installed packages.\n"
        }
        "ps" => "ps - list running processes\nUsage: ps\n",
        "kill" => {
            "kill - terminate a process by pid\nUsage: kill <pid>\n\nRequires being the process owner or root. pid 1 (ag-init) can never be killed.\n"
        }
        "service" => {
            "service - inspect or control a system service\nUsage: service [name] [status|start|stop]\n\nWith no name, lists every service and its state. With a name and no action, reports its status. start/stop require root.\n"
        }
        "logs" => "logs - print the system log\nUsage: logs\n",
        "agpkg" => {
            "agpkg - the AGPKG package manager\nUsage: agpkg <search|info|list|install|remove|update|upgrade> [args]\n\n  search <query>   search the repository by name/description\n  info <name>      show a package's manifest details\n  list             list installed packages\n  install <name>   install a package and its dependencies (root)\n  remove <name>    remove an installed package (root; blocked while something depends on it)\n  update           refresh the package lists (root; flavor only)\n  upgrade          upgrade any outdated installed packages (root)\n"
        }
        "connect" => {
            "connect - authenticate to a remote device\nUsage: connect <host> <username> <password>\n"
        }
        "disconnect" => "disconnect - return to the local device\nUsage: disconnect\n",
        "download" => {
            "download - copy a file off the currently connected remote device\nUsage: download <remote-path> [local-path]\n\nRequires being connected to a remote host. With no local-path, saves under the local $HOME using the remote file's basename. A local-path naming an existing local directory keeps the remote basename inside it; otherwise it's used as the exact destination. This is what completes an ObtainResource contract — 'cat' alone never does.\n"
        }
        "contracts" => {
            "contracts - view or accept jobs\nUsage: contracts [list|accept <id>]\n\nWith no subcommand (or 'list'), shows available/active/completed jobs. 'accept <id>' takes an available job.\n"
        }
        "whois" => {
            "whois - look up public information about an organization\nUsage: whois <organization name>\n\nArguments are joined with a space, so a multi-word name needs no quoting. Free and always available.\n"
        }
        "scan" => {
            "scan - probe a device on the network\nUsage: scan <hostname>\n\nAlways reports reachability. Also lists services and versions once nmap is installed on the device you're currently at.\n"
        }
        "intel" => {
            "intel - review what you've discovered so far\nUsage: intel\n\nLists known hosts, credentials, and observed services recorded during play.\n"
        }
        "help" => {
            "help - list every available command\nUsage: help\n\nRun '<command> --help' or '<command> -h' for a command's full usage.\n"
        }
        "su" => "su - switch to another user on the active device\nUsage: su <user> <password>\n",
        "sudo" => {
            "sudo - run one builtin as root\nUsage: sudo <command> [args...]\n\nPrompts for the caller's password without echoing it. The original identity is restored afterward.\n"
        }
        _ => return format!("{name}: no help available\n"),
    };
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_command_has_help_text() {
        for name in table().keys() {
            let text = help_text(name);
            assert!(
                !text.contains("no help available"),
                "missing help text for '{name}'"
            );
            assert!(
                text.contains("Usage:"),
                "help text for '{name}' has no Usage: line"
            );
        }
    }

    #[test]
    fn help_lists_every_command_with_a_description() {
        let mut shell = super::super::test_support::guest_shell();
        let out = shell.execute_line("help").stdout;
        for name in table().keys() {
            assert!(out.contains(name), "help output is missing '{name}'");
        }
        assert!(out.contains("print the current working directory")); // pwd's description
    }
}
