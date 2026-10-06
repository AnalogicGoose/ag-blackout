# AG Linux — System Architecture

This document tracks the architecture of the AG Linux simulation that powers AG: Blackout, and the incremental development plan. It is a living document — update it as design decisions are made or revised. For the gameplay design behind Phase 7 (why the game plays this way, not just how the code is organized), see [`GAME_DESIGN.md`](./GAME_DESIGN.md).

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
├── world/
│   ├── mod.rs
│   ├── device.rs             # Device (+ CredentialLead — an authored "this path leaks this credential" fact)
│   ├── network.rs             # Network — OS-blind identity registry + reachability
│   └── organization.rs         # Organization, OrganizationRegistry — ownership grouping on top of Network (Slice 2)
├── career/
│   ├── mod.rs
│   ├── economy.rs            # Economy — the player's balance
│   ├── contract.rs            # Contract, ContractStatus, Objective, Lead (Directed/Guided, Slice 2)
│   ├── board.rs                # ContractBoard — accept/record_download/record_modify (resolution + payout)
│   ├── knowledge.rs             # Knowledge, DiscoveredCredential — what the player has learned (Slice 2)
│   └── state.rs                  # Career — the persistent player/progress layer holding Knowledge (Slice 2)
├── shell/
│   ├── mod.rs
│   ├── parser.rs            # tokenizer + pipeline/redirection parsing
│   ├── output.rs             # CommandOutput, LineResult
│   ├── session.rs             # Shell — session (network/economy/contracts/career/identity), execute_line()
│   ├── scenario.rs             # tutorial() — the hardcoded Slice 1 + Slice 2 scenario (content, not engine)
│   ├── test_support.rs          # #[cfg(test)] helpers (guest_shell(), root_shell(), ...)
│   └── builtins/
│       ├── mod.rs                 # the command table (name -> fn) + help_text() (-h/--help usage strings)
│       ├── navigation.rs           # pwd, cd, ls
│       ├── files.rs                 # cat (plain read; also checks CredentialLead), touch, mkdir, rm, cp, mv
│       ├── identity.rs               # whoami, id, groups
│       ├── env.rs                     # echo, env, export, which
│       ├── process.rs                  # ps, kill, service, logs
│       ├── agpkg.rs                     # agpkg search/install/remove/update/upgrade/list/info
│       ├── network.rs                    # connect, disconnect, download (completes ObtainResource contracts on success)
│       ├── contracts.rs                   # contracts, contracts accept <id> (Directed vs. Guided briefings)
│       └── recon.rs                        # whois, scan, intel (Slice 2)
└── ui/
    ├── mod.rs
    ├── terminal.rs          # TerminalGuard (raw mode + alternate screen, restores on drop/panic)
    ├── app.rs                 # App — scrollback/input/cursor/history state, Tab completion, Fish-style autosuggestions, handle_key(), run() event loop
    └── render.rs                # one borderless scrollback pane — reads as a real terminal, not a TUI-with-a-terminal-widget
```

`ui/` is the Ratatui terminal client — Godot will be a separate, unrelated client added much later, not part of this module tree.

## ExecutionContext

Every command runs against an `ExecutionContext` (uid, gids, cwd, environment). `system::registry::UserDatabase::execution_context_for(uid)` builds one from a seeded account. `ExecutionContext::fs_access()` converts it into a `filesystem::FsAccess`, which is all `VirtualFS` needs to evaluate a permission check.

This is a deliberate one-way dependency: **`system` depends on `filesystem`, never the other way around.** `VirtualFS` only ever sees a `FsAccess` (uid + gids + is_superuser) — it has no idea what a `User` or a password is. That's what let Phase 1 (filesystem) be built and fully tested before `system` (users) existed at all, and it's what keeps `sudo`/`su`/`chmod`/`chown` out of the filesystem layer entirely: they're system/shell-layer concerns that produce or consume an `ExecutionContext`/`FsAccess`, never special-cased inside `VirtualFS`.

## Device, Network, and the session split

`world::Device` bundles everything a machine needs — `VirtualFS`, `UserDatabase`, `Sudoers`, `ProcessTable`, `ServiceRegistry`, `PackageManager`, `LogBook` — directly, since AG Linux is currently the only OS. That's a deliberate simplification, not an oversight: introducing an OS trait/enum for exactly one implementor would be premature. When a second OS family becomes real, these fields are what gets pulled out behind that seam; nothing above `Device` needs to change to make that possible, because gameplay code never reaches into a device's internals directly (see docs/GAME_DESIGN.md). `Device` also carries `credential_leads: Vec<CredentialLead>` (Slice 2) — an authored, content-only fact ("reading this exact path leaks this credential"), checked by `Shell::note_credential_leads_at` after a successful `cat`; nothing about `VirtualFS`/`Node` changes to support it, the file being read is perfectly ordinary.

`world::Network` is a plain identity registry (`hostname -> Device`) plus a reachability check — intentionally not modeling routing, segments, or ports yet.

**Slice 2** added `world::Organization`/`OrganizationRegistry` — an ownership grouping on top of `Network` (a name, public/flavor info, and the set of hostnames it owns), stored on `Shell` as its own field (`pub organizations: OrganizationRegistry`, alongside `network`) rather than folded into `Network` itself. It also added `career::Career` (holding `career::Knowledge`) as a distinct field on `Shell` (`pub career: Career`) rather than a bare `Knowledge` field directly on `Shell` — `Shell` stays session-scoped (where the player currently is); `Knowledge` (what the player has learned) lives one layer up, reached the same indirect way `Device`/`Network` are reached (`shell.career.knowledge`, never computed or duplicated by a builtin). See docs/GAME_DESIGN.md's Slice 2 section.

`shell::Shell` is a *session*, not a machine: it owns the player's `Network`, `career::Economy`, `career::ContractBoard`, and the current `ExecutionContext` — plus which device that context is currently authenticated against (`active_hostname`, private; `active_device()`/`active_device_mut()` are the accessors every builtin uses instead of touching filesystem/users/etc. directly). `connect`/`disconnect` swap `active_hostname` and `context` between the session's home device and a remote one, restoring the stashed local identity on disconnect. This is what makes `whoami`/`cat`/`ps`/etc. transparently operate against whichever machine the player is currently on. `Shell::download` is the one operation that deliberately touches both devices at once: it reads a path off the active (remote) device and writes it into the local device — at an explicit local path if the `download` builtin was given one (resolved against `Shell::local_cwd()`, never the remote cwd; if that path is an *existing* local directory, the file lands inside it under the remote file's basename rather than overwriting the directory's own name, matching `cp`/`scp`), otherwise under the local identity's home directory by the remote file's basename — then resolves any `ObtainResource` contract matching `(remote hostname, remote path)` via `ContractBoard::record_download`. A `ModifyResource` contract resolves the opposite way: any successful `write_file` on the active device (currently only reachable via shell redirection — `>`/`>>` in `Shell::write_redirect`) is checked against `ContractBoard::record_modify`, which compares the new bytes to the contract's required content. See docs/GAME_DESIGN.md.

## Terminal quality-of-life

AG: Blackout's terminal is a **gameplay interface**, not an attempt at a fully POSIX-compliant shell or a Fish/Bash clone. Fish is used here purely as a **UX reference** — its autosuggestion and completion feel is worth matching because it's comfortable and legible, not because AG: Blackout is trying to *become* Fish. The bar for every item below is "does this make typing commands more pleasant," not "is this what a real shell does."

**This is terminal-input scope, not core — which is a different line than "CLI vs. Godot."** Godot isn't dropping the terminal; it's the CLI's TTY-style full-screen presentation that goes away, replaced by a desktop environment (window manager, multiple app windows) with a terminal as one graphical window among others. That terminal window will very plausibly want this exact QoL — line editing, history, autosuggestions, completion — a player expects a terminal to behave like one no matter what draws its pixels. So the boundary isn't "the CLI needs this and Godot doesn't"; it's "this is terminal input/editing behavior, not simulation state," and it stays out of `shell::Shell`/`shell::builtins` and the layers below (`system`, `filesystem`, `world`, `career`) either way. `Shell::execute_line` has no idea any of this exists, and should stay that way — none of it is a reason to add a method to `Shell` or a builtin. What *is* CLI-specific and won't carry over as-is is the plumbing this is built on (`crossterm::KeyEvent`, `ratatui::Line`/`Span`, raw-mode ANSI rendering) — a Godot terminal window would reimplement the same editing/completion/suggestion *logic* against Godot's own input and rendering primitives, not reuse `ui::App` directly.

**Already implemented:**

- Line editing: Left/Right cursor movement, Home/End, Backspace, Delete.
- Delete-previous-word on both `Ctrl+Backspace` and `Ctrl+W` (`App::delete_word_before_cursor`) — see "Known simplifications" below for why both are bound.
- Command history: Up/Down navigation, in-memory session-scoped history, consecutive-duplicate suppression (`App::submit` skips pushing an entry identical to the last one).
- Fish-style autosuggestions, in full: history-based, dimmed/gray display, updates live while typing, `Right` at end-of-line accepts, sourced only from previous commands (`App::suggestion`).
- Tab completion: command-name completion from the real `builtins::table()`, file/path completion against the active device's real `VirtualFS`, a listed candidate set on ambiguity, direct completion on a single match (`App::completion_candidates`/`App::handle_tab`).
- `Ctrl+C` clears the current input line — there's no long-running foreground operation to interrupt in this simulation (every command is synchronous), so "cancel the current operation" and "clear pending input" are the same action here.
- Prompt clarity: `user@hostname:cwd$ ` (`App::prompt_string`) always shows identity, host, and cwd — switching from `guest@localhost` to e.g. `dvance@corp-fs01` on `connect` is today's (text-only) local-vs-remote indication.
- Terminal colors: green prompt, red stderr, dim gray autosuggestions, black-on-yellow Scroll Mode status bar (`render.rs`).
- Redraw: the whole frame is repainted from `App` state every event-loop tick (`ui::app::run`) — see "Known simplifications" below for the two known cosmetic edge cases around wrapped lines.
- Resize handling (basic): `run()` re-reads `terminal.size()` and redraws every loop tick (bounded by the 250ms poll timeout), so a resized terminal reflows within that window. There's no dedicated `Event::Resize` handler doing it instantly, but nothing breaks or needs a manual redraw either.
- `Ctrl+A` / `Ctrl+E` — jump to beginning/end of line (Emacs-style bindings alongside Home/End, which already do this).
- `Ctrl+U` / `Ctrl+K` — delete from cursor to beginning/end of line (`App::delete_to_line_start`/`delete_to_line_end`).
- `Ctrl+L` — clears the scrollback (`App::clear_screen`); there's no separate "redraw" step to distinguish since the whole frame repaints from `App` state every tick anyway.
- `~` as shorthand for the home directory in path arguments, expanded in `shell::parser::tokenize` against `env["HOME"]` — only as the first character of a word, followed by `/`/whitespace/end-of-input (so `~someuser` and mid-word `~` are left literal, matching real shells; no per-user home lookup is modeled).
- `Ctrl+R` — incremental reverse history search (`AppMode::ReverseSearch`), bash-style: typing narrows to the most recent history entry containing the query as a substring, `Ctrl+R` again steps to the next older match, `Enter` runs the matched command immediately, `Esc`/`Ctrl+G` restores the pre-search input line. Any other key exits back to `Normal` keeping whatever's currently shown, rather than also applying that key's own effect — a deliberate simplification, along with pressing `Ctrl+R` a second time on an empty query being a no-op instead of stepping through unfiltered history like real bash.
- Argument-aware Tab completion for the five commands where it was practical (`App::argument_completion_candidates`): service names for `service`, not-yet-installed catalog names for `agpkg install`, installed package names for `agpkg remove`, available contract ids for `contracts accept`, and pids for `kill`. Determined by a lightweight whitespace scan of the words before the one being completed (`App::preceding_words`), same non-parser approach as the rest of Tab completion — not by command/position generally, so any other command's arguments still fall back to generic path completion.
- A distinct cyan prompt color while connected to a remote device (`App::is_connected_remotely`, used in both `render.rs`'s live input line and `App::submit`'s scrollback echo) — on top of the existing hostname/cwd text, so it's harder to miss once earlier `connect` output has scrolled out of view.

**Planned QoL** (not yet built; real candidates for later, not a commitment to build all of them): none currently — see "Explicitly out of scope" below for what's deliberately never planned.

**Explicitly out of scope:** full POSIX shell compatibility; real syscalls/TTY/PTY; real process/job control (`Ctrl+Z`/suspension); a shell scripting language; Fish functions/abbreviations; a configurable keybinding system; full Bash/Fish syntax highlighting; fuzzy-finding; "perfect" contextual completion for every command. None of these make the game more playable — they'd only make the terminal harder to reason about for no gameplay benefit, the same reasoning [`GAME_DESIGN.md`](./GAME_DESIGN.md)'s Core gameplay philosophy already applies elsewhere ("systems producing gameplay... not a production shell").

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
- **`sudo`/`su` are simplified vs. real Linux:** `sudo` caches successful authentication for five minutes on the current device and uid, refreshing the deadline on each successful `sudo` invocation and clearing it on a session switch. Bare commands continue to run as the caller. There are no `NOPASSWD` sudoers entries, per-command or per-target-user restrictions, or denied-attempt logs.
- **Hard links (`ln` without `-s`) are not implemented.** `VirtualFS` owns nodes directly in a tree (`BTreeMap<String, Node>` per directory) with no inode-indirection layer, so a hard link (two directory entries sharing one node) isn't representable yet. Symlinks work today because they're just a node holding a target string. Adding hard links means introducing an inode table — postponed until something actually needs it.
- **`su`'s `login` flag is a plain `bool`**, not real `-`/`--login` flag parsing.
- **`su`/`sudo` are shell builtins.** `su` currently accepts an explicit password argument. `sudo <command> [args...]` prompts for the caller's password without echoing it in `ui::App`; verification still uses `system::privilege::sudo`. The interactive password is not part of command history or scrollback.
- **No command chaining (`;`, `&&`, `||`), no globbing, no backslash escaping** in the shell parser — not needed yet, easy to add later without restructuring.
- **`-h`/`--help` is handled once, centrally, in `Shell::run_command`** — it checks whether the command's own args contain either flag and, if so, returns `builtins::help_text(name)` instead of calling the builtin at all, rather than every builtin function checking for it itself. Every registered command gets it "for free"; adding a new builtin without a matching `help_text` entry fails a dedicated test (`every_registered_command_has_help_text`) rather than silently printing a placeholder in the game. Only recognized top-level commands get a help response — `nope --help` still reports "command not found," matching real shells (bash included) rather than printing help for a command that doesn't exist. A bare `help` builtin lists every command with the one-line description taken from its own `help_text` entry (so the two can't drift out of sync) — `help --help` gets intercepted the same as any other command's, not special-cased.
- **`PATH`/`which` are mostly flavor.** `which` only knows about shell builtins, not AGPKG-installed packages — AGPKG doesn't write anything into `VirtualFS` (no real binary files show up under `/usr/bin`), it's purely an in-memory `InstalledDatabase`. Same reasoning as `/proc`/logs below: no read-back path exists yet to make that worthwhile.
- **`/proc` is not synthesized.** Real dynamic procfs content needs a `Node` variant that generates content on read; `NodeKind` only holds static bytes. `ps` covers the actual ask (Phase 5); revisit if something needs `cat /proc/<pid>/...` specifically.
- **System logs (`kill`, `service`, `agpkg`) live in an in-memory `LogBook`, not `/var/log`.** No simulated clock exists to timestamp them meaningfully in-world, and nothing reads log files back off disk yet either.
- **AGPKG's catalog and `/etc/agpkg/repos.conf` are unrelated**, same pattern as `/etc/sudoers`: the repo file written by Phase 1 is flavor text, `package::Repository` is a separate static embedded catalog. No real network fetches (Section 16 says not to implement those anyway).
- **`ui` has its own known gaps:** scroll math treats each logical output line as exactly one terminal row, so a line that word-wraps across multiple rows throws off scroll precision slightly (acceptable for typical short command output, not exact). The live cursor's row position assumes the input line itself doesn't wrap — a very long typed command will visually mis-place the cursor. Both are cosmetic, not correctness bugs in the underlying `Shell`/`App` state.
- **The Scroll Mode toggle fires on `Ctrl+Space`, not strictly `Ctrl+Shift+Space`.** Most terminals — including several IDE-embedded ones — send the same byte for both, so requiring the `SHIFT` modifier made the toggle unreachable there even though it's labeled "Ctrl+Shift+Space" in the status line. `Esc` always exits Scroll Mode too, as a protocol-independent fallback.
- **Delete-previous-word is bound to both `Ctrl+Backspace` and `Ctrl+W`**, same reasoning as the Scroll Mode toggle above: plenty of terminals don't send a distinct sequence for `Ctrl+Backspace` at all, so `Ctrl+W` (the traditional, near-universal readline binding for the same action) is there as the reliable fallback. `App::delete_word_before_cursor` matches readline's `unix-word-rubout`: it eats any whitespace directly at the cursor first, then the word behind it — so from the very end of a multi-word line, repeated presses clear it one word at a time, but a cursor sitting right after a word (not after trailing whitespace) only removes that word, leaving any whitespace before it untouched.
- **`Tab` completion and Fish-style autosuggestions are `App`-level (CLI-only), not `Shell`-level.** `Tab` completes the word under the cursor: command names come straight from `builtins::table()` (the real registry, not a separate list) when it's the line's first word, otherwise directory entries from `shell.active_device().filesystem.list_dir(...)` at the current cwd — a single unambiguous match is inserted directly (trailing space, or a bare `/` for a directory so completion can continue deeper); multiple matches are printed to the scrollback like any other command's output, without touching the input line. This deliberately doesn't reuse `shell::parser::parse` — that parser rejects incomplete/unterminated input by design (see its own doc comment), which is exactly the state the line is normally in while a player is still typing, so completion instead does its own lightweight whitespace-based word-boundary scan (`App::current_word_start`); it only recognizes the very first word of the whole line as "a command name," so a word after a pipe (`|`) is treated as a path, not a command, for now. Autosuggestion (`App::suggestion`) is a pure function of `input`/`history` — no state to keep in sync — returning the remainder of the most recent history entry that starts with (but isn't equal to) the current input, only once the cursor is at the end of the line; `render.rs` shows it as dimmed text after the real input, and `Right` at end-of-line accepts it into the buffer instead of the (otherwise-clamped, no-op) cursor move. Neither feature touches `Shell`/`execute_line` — both operate purely on `App`'s own input/history state plus read-only `Shell` queries, so nothing runs until `Enter` is actually pressed.

## Development phases

1. **Filesystem foundation** — ✅ done. Modular `VirtualFS`, rwx permissions, ownership, hidden files (dot-prefix), symlinks (with loop detection), `touch`/`write_file`/`rm`/`cp`/`mv`, `mkdir`/`remove_dir` (recursive or not), path normalization (`.`/`..`/absolute/relative), the initial AG Linux directory tree, `FsError`, unit tests.
2. **Users** — ✅ done. `User`/`Group`/`UserDatabase` with the four seeded accounts above, `whoami`/`id`/`groups` query logic (`IdInfo` + `Display`), password hashing (FNV-1a, not real crypto — this is a simulation), `ExecutionContext`.
3. **Privileges** — ✅ done. `Sudoers` policy (root + `%sudo` group), `su` (verifies target's password, root bypasses), `sudo` (verifies caller's own password after a sudoers check, keeps caller's cwd). `chmod`/`chown` needed no new work — already correct in `VirtualFS` since Phase 1.
4. **Shell** — ✅ done. Parser (quoting, `$VAR` expansion, pipes, `>`/`>>`/`<`), `Shell::execute_line`, builtins for navigation/files/identity/env.
5. **System** — ✅ done. `ProcessTable` (pid 1 = unkillable `ag-init`, owner-or-root `kill`), `ServiceRegistry` (sshd/cron/nginx, root-gated start/stop tied to a backing process), `ps`/`kill`/`service`/`logs` builtins, in-memory `LogBook`.
6. **AGPKG** — ✅ done. Static embedded `Repository` (8 packages, dependency chains), `InstalledDatabase`, `PackageManager` (dependency-resolving install, dependent-blocked remove, root-gated everything), one `agpkg` builtin with 7 subcommands.
7. **Gameplay systems** — *(Slice 1 playable via `cargo run`, with a real 4-job contract board and two objective kinds)*. `world` (`Device`, `Network`) and `career` (`Contract`, `Objective`, `ContractBoard`, `Economy`) exist; `ContractBoard` has a real `Available → Active → Completed` lifecycle (`post`/`accept(id)`/`available()`/`active()`/`completed()`, `ContractError` for unknown/already-taken ids) instead of contracts arriving pre-accepted. `Shell` is a session that swaps devices via `connect`/`disconnect`; `download` reads a resource off the active remote device and writes it into the local device (home directory by default, or an explicit local path), completing any *active* `ObtainResource` contract targeting that `(hostname, path)` via `ContractBoard::record_download` — `cat` alone is a plain read with no contract effect. A `ModifyResource` contract instead resolves when shell redirection (`>`/`>>`) overwrites the resource on the active device with exactly the required bytes, via `ContractBoard::record_modify`. `shell::scenario::tutorial()` boots the player's machine plus four target devices (one per job — three `ObtainResource`, one `ModifyResource`), posting every contract as `Available` — the player browses with `contracts` (which also shows each job's login) and takes one with `contracts accept <id>`. `ui::App` renders it as one continuous scrollback pane (no borders/widgets — reads as an actual terminal), with line editing, command history (up/down), mouse-wheel scrolling, and a dedicated Scroll Mode (`Ctrl+Space` to toggle — labeled `Ctrl+Shift+Space` in the status line for discoverability, `Esc` always exits) for keyboard-driven scrollback navigation without touching the input line; `main.rs` just wires `TerminalGuard` + `Terminal` + `App::run`. Full vision and rationale in [`GAME_DESIGN.md`](./GAME_DESIGN.md).

8. **Investigation and discovery (Slice 2)** — ✅ done. `world::Organization`/`OrganizationRegistry` (an ownership grouping on top of `Network`); `career::Career` (holding `career::Knowledge`) as its own field on `Shell`, distinct from `Shell`'s session state; `career::contract::Lead` (`Directed`/`Guided`) sitting beside `Objective` — a `Guided` contract's briefing names an `Organization` and a hint instead of a hostname and login, `contracts` never discloses the real `target_hostname` for one. `whois <organization>` (joins its args with a space, so a multi-word name needs no quoting) lists every hostname the org owns and records the org→hostname relationship into `Knowledge`; `scan <hostname>` always reports reachability and, once `nmap` is installed on whichever device the player is currently at, also lists services with name and version (`system::service::Service` gained a `version` field) — information, never a "vulnerable" verdict; `intel` reviews everything `Knowledge` has recorded (hosts, credentials). `world::device::CredentialLead` is a small authored fact ("reading this exact path leaks this credential") checked by `Shell::note_credential_leads_at` after a successful `cat` — nothing about `VirtualFS` changes, the file is ordinary. `shell::scenario::tutorial()` now also registers one `Organization` ("Meridian Analytics") owning two devices connected by exactly one authored password-reuse weakness — the seed device only has the default `guest`/`guest` account, and a note in its home directory leaks the credential that actually unlocks the sibling device holding the contract's resource. Resolution itself is unchanged: `download` still just triggers `ContractBoard::record_download` against the contract's real (now discoverable-in-practice) `target_hostname`. Full rationale in [`GAME_DESIGN.md`](./GAME_DESIGN.md)'s Slice 2 section.

We are inside Phase 7 — both Slice 1 and Slice 2 work end to end via `cargo run`.

**Next:** Slice 3's interactive sudo prompt, `audit-vault01` privilege chain, and five-minute sudo authentication cache are implemented locally; see [`GAME_DESIGN.md`](./GAME_DESIGN.md). Detection, a vulnerability taxonomy, repository tiering, and a generic exploit/objective engine remain later work.
