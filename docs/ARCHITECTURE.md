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
└── system/
    ├── mod.rs
    ├── user.rs              # User, PasswordState (+ FNV-1a hashing)
    ├── group.rs              # Group
    ├── registry.rs            # UserDatabase — seeded accounts, lookups, id/whoami
    ├── context.rs              # ExecutionContext — identity + cwd + env, bridges to FsAccess
    ├── sudoers.rs               # Sudoers — who may sudo
    └── privilege.rs              # su() / sudo() / PrivilegeError
```

`shell/`, `network/`, `package/` and `ui/` don't exist yet — they land in later phases (see below).

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
- **`su`'s `login` flag is a plain `bool`**, not real `-`/`--login` flag parsing, since there's no shell yet to parse flags. Phase 4 just passes the bool through.

## Development phases

1. **Filesystem foundation** — ✅ done. Modular `VirtualFS`, rwx permissions, ownership, hidden files (dot-prefix), symlinks (with loop detection), `touch`/`write_file`/`rm`/`cp`/`mv`, `mkdir`/`remove_dir` (recursive or not), path normalization (`.`/`..`/absolute/relative), the initial AG Linux directory tree, `FsError`, unit tests.
2. **Users** — ✅ done. `User`/`Group`/`UserDatabase` with the four seeded accounts above, `whoami`/`id`/`groups` query logic (`IdInfo` + `Display`), password hashing (FNV-1a, not real crypto — this is a simulation), `ExecutionContext`.
3. **Privileges** — ✅ done. `Sudoers` policy (root + `%sudo` group), `su` (verifies target's password, root bypasses), `sudo` (verifies caller's own password after a sudoers check, keeps caller's cwd). `chmod`/`chown` needed no new work — already correct in `VirtualFS` since Phase 1.
4. **Shell** — *(next)* environment, `PATH`, command lookup, pipes, redirection, parser.
5. **System** — processes, services, `/proc`, `ps`, `kill`, logs.
6. **AGPKG** — package database, repositories, dependencies, package commands.
7. **Gameplay systems** (later) — vulnerabilities, exploits, privilege escalation mechanics, persistence, objectives, missions, narrative.

We are currently starting Phase 4.
