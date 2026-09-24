use crate::filesystem::{FsAccess, FsError, VirtualFS, VirtualPath};

use super::super::output::CommandOutput;
use super::super::session::Shell;
use super::split_flags;

pub fn cat(shell: &mut Shell, args: &[String], stdin: Option<&str>) -> CommandOutput {
    if args.is_empty() {
        return CommandOutput::ok(stdin.unwrap_or_default().to_string());
    }
    let mut out = String::new();
    let mut err = String::new();
    for arg in args {
        let path = match VirtualPath::resolve(&shell.context.cwd, arg) {
            Ok(p) => p,
            Err(e) => {
                err.push_str(&format!("cat: {arg}: {e}\n"));
                continue;
            }
        };
        match shell.filesystem.read_file(&shell.context.fs_access(), &path) {
            Ok(bytes) => out.push_str(&String::from_utf8_lossy(&bytes)),
            Err(e) => err.push_str(&format!("cat: {arg}: {e}\n")),
        }
    }
    CommandOutput { stdout: out, stderr: err.clone(), exit_code: if err.is_empty() { 0 } else { 1 } }
}

pub fn touch(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    run_for_each_path(shell, args, "touch", |fs, access, path| fs.touch(access, path))
}

pub fn mkdir(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let (flags, paths) = split_flags(args);
    let parents = flags.contains(&"-p");
    if paths.is_empty() {
        return CommandOutput::error("mkdir: missing operand\n");
    }
    let mut err = String::new();
    for raw in &paths {
        let path = match VirtualPath::resolve(&shell.context.cwd, raw) {
            Ok(p) => p,
            Err(e) => {
                err.push_str(&format!("mkdir: {raw}: {e}\n"));
                continue;
            }
        };
        let access = shell.context.fs_access();
        let result =
            if parents { mkdir_all(&mut shell.filesystem, &access, &path) } else { shell.filesystem.mkdir(&access, &path) };
        if let Err(e) = result {
            err.push_str(&format!("mkdir: cannot create directory '{raw}': {e}\n"));
        }
    }
    if err.is_empty() {
        CommandOutput::empty_ok()
    } else {
        CommandOutput { stdout: String::new(), stderr: err, exit_code: 1 }
    }
}

/// Like `mkdir -p`: skips prefixes that already exist instead of trying to
/// (re)create them. That distinction matters here — attempting `mkdir` on an
/// existing path checks *write* permission on its parent even though nothing
/// would change, which wrongly denies e.g. a guest creating `/home/guest/x`
/// just because `/home` itself is root-owned.
fn mkdir_all(fs: &mut VirtualFS, access: &FsAccess, path: &VirtualPath) -> Result<(), FsError> {
    let mut built = VirtualPath::root();
    for component in path.components() {
        built = built.join(component);
        match fs.is_dir(access, &built) {
            Ok(true) => continue,
            Ok(false) => return Err(FsError::NotADirectory(built.to_string())),
            Err(FsError::NotFound(_)) => fs.mkdir(access, &built)?,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn rm(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let (flags, paths) = split_flags(args);
    let recursive = flags.iter().any(|f| f.contains('r') || f.contains('R'));
    if paths.is_empty() {
        return CommandOutput::error("rm: missing operand\n");
    }
    let mut err = String::new();
    for raw in &paths {
        let path = match VirtualPath::resolve(&shell.context.cwd, raw) {
            Ok(p) => p,
            Err(e) => {
                err.push_str(&format!("rm: {raw}: {e}\n"));
                continue;
            }
        };
        let access = shell.context.fs_access();
        let result = match shell.filesystem.remove_file(&access, &path) {
            Err(FsError::IsADirectory(_)) if recursive => shell.filesystem.remove_dir(&access, &path, true),
            other => other,
        };
        if let Err(e) = result {
            err.push_str(&format!("rm: {raw}: {e}\n"));
        }
    }
    if err.is_empty() {
        CommandOutput::empty_ok()
    } else {
        CommandOutput { stdout: String::new(), stderr: err, exit_code: 1 }
    }
}

pub fn cp(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    two_path_op(shell, args, "cp", |fs, access, from, to| fs.copy_file(access, from, to))
}

pub fn mv(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    two_path_op(shell, args, "mv", |fs, access, from, to| fs.rename(access, from, to))
}

fn run_for_each_path(
    shell: &mut Shell,
    args: &[String],
    cmd_name: &str,
    op: impl Fn(&mut VirtualFS, &FsAccess, &VirtualPath) -> Result<(), FsError>,
) -> CommandOutput {
    if args.is_empty() {
        return CommandOutput::error(format!("{cmd_name}: missing operand\n"));
    }
    let mut err = String::new();
    for raw in args {
        let path = match VirtualPath::resolve(&shell.context.cwd, raw) {
            Ok(p) => p,
            Err(e) => {
                err.push_str(&format!("{cmd_name}: {raw}: {e}\n"));
                continue;
            }
        };
        let access = shell.context.fs_access();
        if let Err(e) = op(&mut shell.filesystem, &access, &path) {
            err.push_str(&format!("{cmd_name}: {raw}: {e}\n"));
        }
    }
    if err.is_empty() {
        CommandOutput::empty_ok()
    } else {
        CommandOutput { stdout: String::new(), stderr: err, exit_code: 1 }
    }
}

fn two_path_op(
    shell: &mut Shell,
    args: &[String],
    cmd_name: &str,
    op: impl Fn(&mut VirtualFS, &FsAccess, &VirtualPath, &VirtualPath) -> Result<(), FsError>,
) -> CommandOutput {
    if args.len() != 2 {
        return CommandOutput::error(format!("{cmd_name}: missing file operand\n"));
    }
    let from = match VirtualPath::resolve(&shell.context.cwd, &args[0]) {
        Ok(p) => p,
        Err(e) => return CommandOutput::error(format!("{cmd_name}: {e}\n")),
    };
    let to = match VirtualPath::resolve(&shell.context.cwd, &args[1]) {
        Ok(p) => p,
        Err(e) => return CommandOutput::error(format!("{cmd_name}: {e}\n")),
    };
    let access = shell.context.fs_access();
    match op(&mut shell.filesystem, &access, &from, &to) {
        Ok(()) => CommandOutput::empty_ok(),
        Err(e) => CommandOutput::error(format!("{cmd_name}: {e}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;

    #[test]
    fn cat_reads_file_content() {
        let mut shell = guest_shell();
        shell.execute_line("echo hello > /home/guest/note.txt");
        let result = shell.execute_line("cat /home/guest/note.txt");
        assert_eq!(result.stdout, "hello\n");
    }

    #[test]
    fn cat_missing_file_reports_error() {
        let mut shell = guest_shell();
        let result = shell.execute_line("cat /home/guest/nope.txt");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("nope.txt"));
    }

    #[test]
    fn touch_creates_an_empty_file() {
        let mut shell = guest_shell();
        shell.execute_line("touch /home/guest/new.txt");
        assert_eq!(shell.execute_line("cat /home/guest/new.txt").stdout, "");
    }

    #[test]
    fn mkdir_dash_p_creates_intermediate_directories() {
        let mut shell = guest_shell();
        let result = shell.execute_line("mkdir -p /home/guest/a/b/c");
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.execute_line("cd /home/guest/a/b/c").exit_code, 0);
    }

    #[test]
    fn rm_dir_without_flag_fails_but_dash_r_works() {
        let mut shell = guest_shell();
        shell.execute_line("mkdir /home/guest/dir");
        shell.execute_line("touch /home/guest/dir/file.txt");
        let plain = shell.execute_line("rm /home/guest/dir");
        assert_eq!(plain.exit_code, 1);
        let recursive = shell.execute_line("rm -r /home/guest/dir");
        assert_eq!(recursive.exit_code, 0);
    }

    #[test]
    fn cp_duplicates_a_file() {
        let mut shell = guest_shell();
        shell.execute_line("echo payload > /home/guest/src.txt");
        shell.execute_line("cp /home/guest/src.txt /home/guest/dst.txt");
        assert_eq!(shell.execute_line("cat /home/guest/dst.txt").stdout, "payload\n");
    }

    #[test]
    fn mv_renames_a_file() {
        let mut shell = guest_shell();
        shell.execute_line("touch /home/guest/old.txt");
        shell.execute_line("mv /home/guest/old.txt /home/guest/new.txt");
        assert_eq!(shell.execute_line("cat /home/guest/old.txt").exit_code, 1);
        assert_eq!(shell.execute_line("cat /home/guest/new.txt").exit_code, 0);
    }
}
