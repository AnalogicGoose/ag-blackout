# AG: Blackout

**AG: Blackout** is a terminal/TUI hacking simulator built in Rust with [Ratatui](https://ratatui.rs/) and [Crossterm](https://github.com/crossterm-rs/crossterm).

The game simulates **AG Linux**, a fictional Unix-like operating system developed by the in-universe vendor **AnalogicGoose**. The goal is for the player to feel like they are operating a real Linux machine — filesystem, shell, users, permissions, processes, services, networking and package management — while the entire environment is a self-contained gameplay simulation, not a real OS.

## Status

Early development, built incrementally phase by phase. Done so far: the filesystem foundation (`VirtualFS`, permissions, symlinks, the AG Linux directory tree), users and groups (`UserDatabase`, seeded accounts), and privilege escalation (`sudo`/`su`/sudoers). Next up: the shell. See [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for the module layout, design decisions and full phase breakdown. No story, missions or narrative content yet — this stage is exclusively the technical sandbox.

## Building

```bash
cargo check
cargo test
cargo run
```

## License

MIT — see [`LICENSE`](./LICENSE).
