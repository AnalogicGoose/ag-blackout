mod agpkg;
mod env;
mod files;
mod identity;
mod navigation;
mod network;
mod process;

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
    m
}
