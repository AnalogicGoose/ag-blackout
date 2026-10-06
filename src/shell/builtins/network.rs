use crate::filesystem::VirtualPath;

use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn connect(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let [host, username, password] = args else {
        return CommandOutput::error("connect: usage: connect <host> <username> <password>\n");
    };
    match shell.connect(host, username, password) {
        Ok(()) => CommandOutput::ok(format!("Connected to {host}.\n")),
        Err(e) => CommandOutput::error(format!("connect: {e}\n")),
    }
}

pub fn disconnect(shell: &mut Shell, _args: &[String], _stdin: Option<&str>) -> CommandOutput {
    match shell.disconnect() {
        Ok(()) => CommandOutput::ok("Disconnected.\n"),
        Err(e) => CommandOutput::error(format!("disconnect: {e}\n")),
    }
}

pub fn download(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let (raw_remote, raw_local) = match args {
        [remote] => (remote, None),
        [remote, local] => (remote, Some(local)),
        _ => return CommandOutput::error("download: usage: download <remote-path> [local-path]\n"),
    };
    let remote_path = match VirtualPath::resolve(&shell.context.cwd, raw_remote) {
        Ok(p) => p,
        Err(e) => return CommandOutput::error(format!("download: {raw_remote}: {e}\n")),
    };
    let local_path = match raw_local {
        Some(raw) => match VirtualPath::resolve(shell.local_cwd(), raw) {
            Ok(p) => Some(p),
            Err(e) => return CommandOutput::error(format!("download: {raw}: {e}\n")),
        },
        None => None,
    };
    match shell.download(&remote_path, local_path.as_ref()) {
        Ok(saved) => CommandOutput::ok(format!("Downloaded {raw_remote} to {saved}.\n")),
        Err(e) => CommandOutput::error(format!("download: {e}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;
    use crate::world::Device;

    #[test]
    fn connect_and_disconnect_round_trip() {
        let mut shell = guest_shell();
        shell.network.register(Device::new("target01"));

        let result = shell.execute_line("connect target01 guest guest");
        assert_eq!(result.exit_code, 0);
        assert!(shell.is_connected_remotely());

        let result = shell.execute_line("disconnect");
        assert_eq!(result.exit_code, 0);
        assert!(!shell.is_connected_remotely());
    }

    #[test]
    fn connect_to_unknown_host_fails() {
        let result = guest_shell().execute_line("connect nope guest guest");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("no route to host"));
    }

    #[test]
    fn connect_with_wrong_credentials_fails() {
        let mut shell = guest_shell();
        shell.network.register(Device::new("target01"));
        let result = shell.execute_line("connect target01 guest wrongpass");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("authentication failed"));
    }

    #[test]
    fn disconnect_without_connecting_fails() {
        let result = guest_shell().execute_line("disconnect");
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn download_without_connecting_fails() {
        let result = guest_shell().execute_line("download /home/guest/x.txt");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("not connected"));
    }

    #[test]
    fn download_pulls_the_remote_file_home_and_pays_out_the_contract() {
        use crate::career::Contract;
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = Device::new("target01");
        let path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target
            .filesystem
            .write_file(&FsAccess::root(), &path, b"confidential")
            .unwrap();
        shell.network.register(target);

        let id = shell.contracts.post(Contract::directed(
            "Get the report",
            "target01",
            path,
            3000,
            "guest",
            "guest",
            crate::career::Objective::ObtainResource,
        ));
        shell.execute_line(&format!("contracts accept {id}"));
        shell.execute_line("connect target01 guest guest");

        let result = shell.execute_line("download /home/guest/report.pdf");
        assert_eq!(result.exit_code, 0);
        assert_eq!(shell.economy.balance(), 3000);
        assert_eq!(shell.contracts.completed().count(), 1);

        shell.execute_line("disconnect");
        assert_eq!(
            shell.execute_line("cat /home/guest/report.pdf").stdout,
            "confidential"
        );
    }

    #[test]
    fn download_with_a_local_path_argument_saves_there_instead() {
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = Device::new("target01");
        let path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target
            .filesystem
            .write_file(&FsAccess::root(), &path, b"confidential")
            .unwrap();
        shell.network.register(target);
        shell.execute_line("connect target01 guest guest");

        let result = shell.execute_line("download /home/guest/report.pdf /tmp/loot.pdf");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("/tmp/loot.pdf"));

        shell.execute_line("disconnect");
        assert_eq!(
            shell.execute_line("cat /tmp/loot.pdf").stdout,
            "confidential"
        );
    }

    #[test]
    fn downloading_into_an_existing_directory_keeps_the_remote_filename() {
        use crate::filesystem::{FsAccess, VirtualPath};

        let mut shell = guest_shell();
        let mut target = Device::new("target01");
        let path = VirtualPath::resolve(&VirtualPath::root(), "/home/guest/report.pdf").unwrap();
        target
            .filesystem
            .write_file(&FsAccess::root(), &path, b"confidential")
            .unwrap();
        shell.network.register(target);
        shell.execute_line("connect target01 guest guest");

        let result = shell.execute_line("download /home/guest/report.pdf /tmp");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("/tmp/report.pdf"));

        shell.execute_line("disconnect");
        assert_eq!(
            shell.execute_line("cat /tmp/report.pdf").stdout,
            "confidential"
        );
    }
}
