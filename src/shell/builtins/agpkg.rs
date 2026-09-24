use super::super::output::CommandOutput;
use super::super::session::Shell;

pub fn agpkg(shell: &mut Shell, args: &[String], _stdin: Option<&str>) -> CommandOutput {
    let Some(subcommand) = args.first() else {
        return CommandOutput::error("agpkg: missing subcommand (search/install/remove/update/upgrade/list/info)\n");
    };
    let rest = &args[1..];
    let is_root = shell.context.is_root();
    let uid = shell.context.uid;

    match subcommand.as_str() {
        "search" => {
            let Some(query) = rest.first() else {
                return CommandOutput::error("agpkg search: missing query\n");
            };
            let out = shell.packages.search(query).iter().map(|p| format!("{} - {}", p.name, p.description)).collect::<Vec<_>>().join("\n");
            CommandOutput::ok(if out.is_empty() { out } else { out + "\n" })
        }
        "info" => {
            let Some(name) = rest.first() else {
                return CommandOutput::error("agpkg info: missing package name\n");
            };
            match shell.packages.info(name) {
                Some(p) => CommandOutput::ok(format!(
                    "Package: {}\nVersion: {}\nDescription: {}\nDepends: {}\n",
                    p.name,
                    p.version,
                    p.description,
                    if p.dependencies.is_empty() { "none".to_string() } else { p.dependencies.join(", ") }
                )),
                None => CommandOutput::error(format!("agpkg info: package '{name}' not found\n")),
            }
        }
        "list" => {
            let mut installed = shell.packages.list_installed();
            installed.sort_by(|a, b| a.name.cmp(&b.name));
            let out = installed
                .iter()
                .map(|p| format!("{}/{}{}", p.name, p.version, if p.explicit { "" } else { " [auto]" }))
                .collect::<Vec<_>>()
                .join("\n");
            CommandOutput::ok(if out.is_empty() { out } else { out + "\n" })
        }
        "install" => {
            let Some(name) = rest.first() else {
                return CommandOutput::error("agpkg install: missing package name\n");
            };
            match shell.packages.install(is_root, name) {
                Ok(installed) => {
                    shell.logs.record("agpkg", format!("installed {} (uid {uid})", installed.join(", ")));
                    CommandOutput::ok(format!("Installing: {}\n", installed.join(", ")))
                }
                Err(e) => CommandOutput::error(format!("agpkg install: {e}\n")),
            }
        }
        "remove" => {
            let Some(name) = rest.first() else {
                return CommandOutput::error("agpkg remove: missing package name\n");
            };
            match shell.packages.remove(is_root, name) {
                Ok(()) => {
                    shell.logs.record("agpkg", format!("removed {name} (uid {uid})"));
                    CommandOutput::empty_ok()
                }
                Err(e) => CommandOutput::error(format!("agpkg remove: {e}\n")),
            }
        }
        "update" => match shell.packages.update(is_root) {
            Ok(()) => CommandOutput::ok("Reading package lists... Done\n"),
            Err(e) => CommandOutput::error(format!("agpkg update: {e}\n")),
        },
        "upgrade" => match shell.packages.upgrade(is_root) {
            Ok(upgraded) if upgraded.is_empty() => CommandOutput::ok("0 upgraded, 0 newly installed\n"),
            Ok(upgraded) => {
                shell.logs.record("agpkg", format!("upgraded {} (uid {uid})", upgraded.join(", ")));
                CommandOutput::ok(format!("Upgraded: {}\n", upgraded.join(", ")))
            }
            Err(e) => CommandOutput::error(format!("agpkg upgrade: {e}\n")),
        },
        other => CommandOutput::error(format!("agpkg: unknown subcommand '{other}'\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::guest_shell;

    #[test]
    fn search_finds_matching_packages() {
        let out = guest_shell().execute_line("agpkg search crack").stdout;
        assert!(out.contains("hydra"));
    }

    #[test]
    fn info_reports_manifest_details() {
        let out = guest_shell().execute_line("agpkg info nmap").stdout;
        assert!(out.contains("Depends: openssl"));
    }

    #[test]
    fn install_requires_root() {
        let result = guest_shell().execute_line("agpkg install nmap");
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("root"));
    }

    #[test]
    fn install_as_root_pulls_in_dependencies_and_lists_them() {
        let mut shell = guest_shell();
        shell.context.uid = 0;
        let result = shell.execute_line("agpkg install nmap");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "Installing: openssl, nmap\n");
        let list = shell.execute_line("agpkg list").stdout;
        assert!(list.contains("nmap/7.94"));
        assert!(list.contains("openssl/3.0.2 [auto]"));
    }

    #[test]
    fn remove_blocked_by_dependent_then_succeeds_after_it_is_gone() {
        let mut shell = guest_shell();
        shell.context.uid = 0;
        shell.execute_line("agpkg install nmap");
        let blocked = shell.execute_line("agpkg remove openssl");
        assert_eq!(blocked.exit_code, 1);
        assert!(blocked.stderr.contains("required by nmap"));
        shell.execute_line("agpkg remove nmap");
        assert_eq!(shell.execute_line("agpkg remove openssl").exit_code, 0);
    }

    #[test]
    fn update_and_upgrade_require_root() {
        let mut shell = guest_shell();
        assert_eq!(shell.execute_line("agpkg update").exit_code, 1);
        assert_eq!(shell.execute_line("agpkg upgrade").exit_code, 1);
        shell.context.uid = 0;
        assert_eq!(shell.execute_line("agpkg update").stdout, "Reading package lists... Done\n");
        assert_eq!(shell.execute_line("agpkg upgrade").stdout, "0 upgraded, 0 newly installed\n");
    }
}
