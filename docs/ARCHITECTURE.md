# AG Linux — System Architecture

This document tracks the target architecture for the AG Linux simulation that powers AG: Blackout, and the incremental development plan. It is a living document — update it as design decisions are made or revised.

## Core principle

AG Linux should feel like a real Unix/Linux operating system from the player's perspective, but it is a gameplay-oriented simulation. We do not implement a real kernel, syscalls, ext4, real process isolation, or real network infrastructure — all of it is simulated in Rust.

## Current scope

Technical foundation only: filesystem, shell, users, permissions, processes, services, networking, package management, terminal interaction, system state. Story, lore, missions, factions and narrative are explicitly out of scope until the sandbox is solid (Phase 7+).

## Module layout

```text
src/
├── main.rs
├── app/            # global application/UI state, active-fs selection
├── filesystem/      # VirtualFS, nodes, metadata, permissions, paths, errors
├── shell/           # parser, commands, execution wiring
├── system/           # users, groups, processes, services, ExecutionContext
├── network/          # NetworkManager, RemoteNode
├── package/          # AGPKG
└── ui/                # Ratatui/Crossterm rendering
```

## ExecutionContext

Every command runs against an `ExecutionContext` (user, uid, groups, cwd, environment, privileges). Permission checks live in the filesystem layer and are evaluated against this context — `sudo`/`su`/`chmod`/`chown` are shell/system-layer concerns, not filesystem special cases. No command hardcodes a permission bypass.

## AG Linux identity

- OS name: `AG Linux 1.0.0 (Blackbird)`
- Vendor: AnalogicGoose
- Arch: x86_64
- `uname -a` style string: `Linux <host> 6.8.12-ag #1 SMP x86_64 GNU/Linux`

## Development phases

1. **Filesystem foundation** — modular VirtualFS, permissions, ownership, hidden files, symlinks, `touch`/`rm`/`cp`/`mv`, file sizes, detailed `ls`, path handling, initial directory tree, tests. *(in progress)*
2. **Users** — users, groups, UID/GID, passwords, `whoami`/`id`/`groups`.
3. **Privileges** — `sudo`, `su`, sudoers, `chmod`, `chown`.
4. **Shell** — environment, `PATH`, command lookup, pipes, redirection, parser improvements.
5. **System** — processes, services, `/proc`, `ps`, `kill`, logs.
6. **AGPKG** — package database, repositories, dependencies, package commands.
7. **Gameplay systems** (later) — vulnerabilities, exploits, privilege escalation mechanics, persistence, objectives, missions, narrative.

We are currently in Phase 1.
