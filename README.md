# AG: Blackout

**AG: Blackout** is a terminal/TUI hacking simulator built in Rust with [Ratatui](https://ratatui.rs/) and [Crossterm](https://github.com/crossterm-rs/crossterm).

The game simulates **AG Linux**, a fictional Unix-like operating system developed by the in-universe vendor **AnalogicGoose**. The goal is for the player to feel like they are operating a real Linux machine — filesystem, shell, users, permissions, processes, services, networking and package management — while the entire environment is a self-contained gameplay simulation, not a real OS.

## Status

Early development. The project currently ships a minimal stub; the OS simulation (filesystem, shell, users/permissions, processes, services, networking, AGPKG) is being built incrementally. See [`ARCHITECTURE.md`](./ARCHITECTURE.md) for the target module layout and development phases. No story, missions or narrative content yet — this stage is exclusively the technical sandbox.

## Building

```bash
cargo check
cargo test
cargo run
```

## License

MIT — see [`LICENSE`](./LICENSE).
