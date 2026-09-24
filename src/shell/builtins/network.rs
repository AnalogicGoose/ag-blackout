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
}
