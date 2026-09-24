# AG Linux — System Architecture

This document tracks the architecture of the AG Linux simulation that powers AG: Blackout, and the incremental development plan. It is a living document — update it as design decisions are made or revised.

## Core principle

AG Linux should feel like a real Unix/Linux operating system from the player's perspective, but it is a gameplay-oriented simulation. We do not implement a real kernel, syscalls, ext4, real process isolation, or real network infrastructure — all of it is simulated in Rust.

## Current scope

Technical foundation only: filesystem, shell, users, permissions, processes, services, networking, package management, terminal interaction, system state. Story, lore, missions, factions and narrative are explicitly out of scope until the sandbox is solid (Phase 7+).

## Module layout

The project is a library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`). Each module is a folder with a `mod.rs` entry point, keeping `src/` itself uncluttered:

```text
src/
├── main.rs
├── lib.rs
├── filesystem/
│   ├── mod.rs          # re-exports the module's public API
│   ├── vfs.rs           # VirtualFS — the tree, path resolution, all operations
│   ├── node.rs           # Node / NodeKind (File, Directory, Symlink)
│   ├── metadata.rs        # Metadata (mode, owner, group, timestamps)
│   ├── permissions.rs      # Mode (rwx bits), FsAccess, AccessClass/AccessMode
│   ├── path.rs              # VirtualPath — absolute, normalized paths
│   └── error.rs              # FsError
├── system/
│   ├── mod.rs
│   ├── user.rs              # User, PasswordState (+ FNV-1a hashing)
│   ├── group.rs              # Group
│   ├── registry.rs            # UserDatabase — seeded accounts, lookups, id/whoami
│   ├── context.rs              # ExecutionContext — identity + cwd + env, bridges to FsAccess
│   ├── sudoers.rs               # Sudoers — who may sudo
│   ├── privilege.rs              # su() / sudo() / PrivilegeError
│   ├── process.rs                 # Process, ProcessTable (spawn/kill, pid 1 = ag-init)
│   ├── service.rs                  # Service, ServiceRegistry (start/stop, root-gated)
│   └── log.rs                       # LogEntry, LogBook — in-memory only, see below
├── package/
│   ├── mod.rs
│   ├── manifest.rs          # PackageManifest (name, version, description, deps)
│   ├── repository.rs         # Repository — the static embedded AGPKG catalog
│   ├── installed.rs           # InstalledPackage, InstalledDatabase
│   └── manager.rs               # PackageManager — install/remove/update/upgrade
└── shell/
    ├── mod.rs
    ├── parser.rs            # tokenizer + pipeline/redirection parsing
    ├── output.rs             # CommandOutput, LineResult
    ├── session.rs             # Shell — owns everything, execute_line() is the entry point
    ├── test_support.rs         # #[cfg(test)] helpers (guest_shell(), root_shell(), ...)
    └── builtins/
        ├── mod.rs                # the command table (name -> fn)
        ├── navigation.rs          # pwd, cd, ls
        ├── files.rs                # cat, touch, mkdir, rm, cp, mv
        ├── identity.rs              # whoami, id, groups
        ├── env.rs                    # echo, env, export, which
        ├── process.rs                  # ps, kill, service, logs
        └── agpkg.rs                      # agpkg search/install/remove/update/upgrade/list/info
```

`network/` and `ui/` don't exist yet — they land in later phases (see below).

## ExecutionContext

Every command runs against an `ExecutionContext` (uid, gids, cwd, environment). `system::registry::UserDatabase::execution_context_for(uid)` builds one from a seeded account. `ExecutionContext::fs_access()` converts it into a `filesystem::FsAccess`, which is all `VirtualFS` needs to evaluate a permission check.

This is a deliberate one-way dependency: **`system` depends on `filesystem`, never the other way around.** `VirtualFS` only ever sees a `FsAccess` (uid + gids + is_superuser) — it has no idea what a `User` or a password is. That's what let Phase 1 (filesystem) be built and fully tested before `system` (users) existed at all, and it's what keeps `sudo`/`su`/`chmod`/`chown` out of the filesystem layer entirely: they're system/shell-layer concerns that produce or consume an `ExecutionContext`/`FsAccess`, never special-cased inside `VirtualFS`.

## AG Linux identity

- OS name: `AG Linux 1.0.0 (Blackbird)`
- Vendor: AnalogicGoose
- Arch: x86_64
- `uname -a` style string: `Linux <host> 6.8.12-ag #1 SMP x86_64 GNU/Linux`

## Default accounts

Seeded by `UserDatabase::new()`:

| user       | uid  | primary group | supplementary groups | password        |
|------------|------|----------------|------------------------|------------------|
| `root`     | 0    | `root` (0)     | —                        | locked           |
| `admin`    | 1000 | `admin` (1000) | `sudo` (27)               | `admin123`         |
| `guest`    | 1001 | `guest` (1001) | —                          | `guest`             |
| `www-data` | 33   | `www-data` (33)| —                          | locked (no login)     |

## Known simplifications / postponed on purpose

- **`/etc/passwd`, `/etc/shadow`, `/etc/sudoers` are static flavor text**, written once by `VirtualFS::build_default_tree()` (Phase 1). They are *not* generated from, or read back into, `UserDatabase`/`Sudoers` (Phase 2/3). The two are independent right now. This matters for a hacking sim specifically: classic privilege-escalation techniques like "edit `/etc/sudoers` to grant yourself access" or "find a misconfigured `NOPASSWD` line" would require actually parsing those files into the real policy structs instead of hardcoding it in `Sudoers`. Worth revisiting once there's a concrete gameplay reason to.
- **`sudo`/`su` are simplified vs. real Linux:** no password caching (real `sudo` remembers ~15 min per session), no `NOPASSWD` sudoers entries, no per-command or per-target-user restriction (ours is all-or-nothing once permitted), no logging of denied attempts. All reasonable Phase 7 (gameplay) or later hooks.
- **Hard links (`ln` without `-s`) are not implemented.** `VirtualFS` owns nodes directly in a tree (`BTreeMap<String, Node>` per directory) with no inode-indirection layer, so a hard link (two directory entries sharing one node) isn't representable yet. Symlinks work today because they're just a node holding a target string. Adding hard links means introducing an inode table — postponed until something actually needs it.
- **`su`'s `login` flag is a plain `bool`**, not real `-`/`--login` flag parsing.
- **`su`/`sudo` are still not shell builtins.** They need an interactive password prompt (hidden input, multi-turn), which has nowhere to live without a UI loop — Ratatui/Crossterm are still completely unused; `main.rs` only prints the boot banner. Revisit once there's an actual terminal loop.
- **No command chaining (`;`, `&&`, `||`), no globbing, no backslash escaping** in the shell parser — not needed yet, easy to add later without restructuring.
- **`PATH`/`which` are mostly flavor.** `which` only knows about shell builtins, not AGPKG-installed packages — AGPKG doesn't write anything into `VirtualFS` (no real binary files show up under `/usr/bin`), it's purely an in-memory `InstalledDatabase`. Same reasoning as `/proc`/logs below: no read-back path exists yet to make that worthwhile.
- **`/proc` is not synthesized.** Real dynamic procfs content needs a `Node` variant that generates content on read; `NodeKind` only holds static bytes. `ps` covers the actual ask (Phase 5); revisit if something needs `cat /proc/<pid>/...` specifically.
- **System logs (`kill`, `service`, `agpkg`) live in an in-memory `LogBook`, not `/var/log`.** No simulated clock exists to timestamp them meaningfully in-world, and nothing reads log files back off disk yet either.
- **AGPKG's catalog and `/etc/agpkg/repos.conf` are unrelated**, same pattern as `/etc/sudoers`: the repo file written by Phase 1 is flavor text, `package::Repository` is a separate static embedded catalog. No real network fetches (Section 16 says not to implement those anyway).

## Development phases

1. **Filesystem foundation** — ✅ done. Modular `VirtualFS`, rwx permissions, ownership, hidden files (dot-prefix), symlinks (with loop detection), `touch`/`write_file`/`rm`/`cp`/`mv`, `mkdir`/`remove_dir` (recursive or not), path normalization (`.`/`..`/absolute/relative), the initial AG Linux directory tree, `FsError`, unit tests.
2. **Users** — ✅ done. `User`/`Group`/`UserDatabase` with the four seeded accounts above, `whoami`/`id`/`groups` query logic (`IdInfo` + `Display`), password hashing (FNV-1a, not real crypto — this is a simulation), `ExecutionContext`.
3. **Privileges** — ✅ done. `Sudoers` policy (root + `%sudo` group), `su` (verifies target's password, root bypasses), `sudo` (verifies caller's own password after a sudoers check, keeps caller's cwd). `chmod`/`chown` needed no new work — already correct in `VirtualFS` since Phase 1.
4. **Shell** — ✅ done. Parser (quoting, `$VAR` expansion, pipes, `>`/`>>`/`<`), `Shell::execute_line`, builtins for navigation/files/identity/env.
5. **System** — ✅ done. `ProcessTable` (pid 1 = unkillable `ag-init`, owner-or-root `kill`), `ServiceRegistry` (sshd/cron/nginx, root-gated start/stop tied to a backing process), `ps`/`kill`/`service`/`logs` builtins, in-memory `LogBook`.
6. **AGPKG** — ✅ done. Static embedded `Repository` (8 packages, dependency chains), `InstalledDatabase`, `PackageManager` (dependency-resolving install, dependent-blocked remove, root-gated everything), one `agpkg` builtin with 7 subcommands.
7. **Gameplay systems** (later) — vulnerabilities, exploits, privilege escalation mechanics, persistence, objectives, missions, narrative.

We are currently starting Phase 7.
