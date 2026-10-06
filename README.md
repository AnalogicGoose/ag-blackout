# AG: Blackout

**AG: Blackout** is a terminal/TUI hacking simulator built in Rust with [Ratatui](https://ratatui.rs/) and [Crossterm](https://github.com/crossterm-rs/crossterm).

The game simulates **AG Linux**, a fictional Unix-like operating system developed by the in-universe vendor **AnalogicGoose**. The goal is for the player to feel like they are operating a real Linux machine — filesystem, shell, users, permissions, processes, services, networking and package management — while the entire environment is a self-contained gameplay simulation, not a real OS.

## Status

The technical foundation is complete: filesystem (`VirtualFS`), users/groups, privilege escalation (`sudo`/`su`/sudoers), a real shell (parser, pipes, redirection, builtins), processes/services (`ps`/`kill`/`service`), and the AGPKG package manager. See [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) for the module layout and phase breakdown.

Gameplay design (contracts, economy, the Device/OS/Network model) is documented in [`docs/GAME_DESIGN.md`](./docs/GAME_DESIGN.md). Slices 1–4 are playable: the contract board includes direct jobs, an investigation chain, a privilege escalation chain, and a service foothold. Run `cargo run` to open the full-screen terminal. Use `tutorial` to see the guided Terminal lesson, `tutorial start terminal` to play it, `help` to browse commands, and `clear` to empty the scrollback. The onboarding rework is in progress; contracts are still available without completing lessons.

## Building

```bash
cargo check
cargo test
cargo run
```

## License

MIT — see [`LICENSE`](./LICENSE).
