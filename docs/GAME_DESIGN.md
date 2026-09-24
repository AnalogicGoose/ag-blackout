# AG: Blackout — Game Design

This document captures the gameplay design decisions made before Phase 7 implementation begins. It complements [`ARCHITECTURE.md`](./ARCHITECTURE.md), which covers the technical module layout — this one covers *why* the game plays the way it does. Living document; update it as design decisions are made or revised.

## Vision: two real games, one shared foundation

AG: Blackout ships as a CLI/TUI game first (Rust, Ratatui/Crossterm). That CLI version is **a complete, playable game in its own right** — not a throwaway prototype for a "real" future version.

A larger, more ambitious graphical version is planned for Godot later. It's a bigger game (visual network topology, multiple windows/panels, file browsers, contract boards, per-software interfaces, system monitoring, richer contextual interaction) — but it is not a different game underneath. Both clients present the same simulation:

```text
                 AG: Blackout Core
              Simulation + Gameplay state and rules
                       │
             ┌─────────┴─────────┐
             │                   │
        CLI Client          Godot Client
         (first, real)       (future, bigger)
             │                   │
       Text interaction     Graphical UI
```

**Design test for every new system:** if the CLI disappeared tomorrow and Godot became the only client, would this system still make sense? If yes, it belongs in the core. If it only exists because a terminal needs to parse or display something, it belongs in the CLI client layer.

Applying that test to what already exists: `filesystem`, `system` (users/privilege/process/service/log), `package` are core — a graphical file browser or job board needs exactly that data. `shell`'s parser, builtins, and `execute_line`/`CommandOutput` are CLI-client-specific — a graphical file browser would call `VirtualFS::list_dir` directly, never touching a parser. They'd only be reused by a future Godot *terminal-emulator widget* specifically, not by other Godot UI.

## Core gameplay philosophy

Inspired primarily by **Uplink** (contracts, money, purchasing better software, progression through equipment) and **Grey Hack** (sandbox freedom, systemic interaction, player-determined solutions) — without copying either.

> Give the player an objective, provide a simulated digital environment and tools, and let the player determine how to accomplish it.

The synthesis: the **contract board is the macro loop** (money, reputation, unlocks — gives the player direction and legible progress). **Sandbox freedom is the micro loop, inside each contract** — a contract defines a goal state and a small persistent network around it; the player solves it however they want, and the engine only checks whether the goal state became true.

The world should persist beyond individual contracts. If the player finds something unrelated to the current contract while poking around a target, that's real, reusable state — not scripted. This is deliberate: **systems producing gameplay**, not missions pretending to be systemic.

## Contracts

Contracts describe **outcomes**, not procedures. Bad: "1. Run nmap. 2. Exploit port 22. 3. sudo. 4. Copy the file." Preferred: "Obtain `report.pdf` from target X. Reward: $3,000" — the player decides how.

General shape (not all fields required per contract):

```text
Contract
├── Objectives
├── Constraints
├── Targets / Entities involved (optional — zero, one, or multiple)
└── Resolution rules
```

`target` is optional because not every contract is device-shaped. A programming contract can be a standalone task ("build a utility meeting these requirements") with no fake computer to justify it. A networking contract might involve multiple devices at once ("diagnose why the web service can't reach the database server"). The contract model shouldn't be designed around exactly one target device.

**Objective types** will eventually include `ReadResource`, `CopyResource`, `ModifyResource`, `DeleteResource`, `ExecuteAction`, `GainAccess`, etc. — a real extensible objective system, later. For the first vertical slice, exactly one objective kind is hardcoded: `ObtainResource`, meaning "authenticate to the target device and read the specified resource" (proof-of-access — see Slice 1 below). The implementation shouldn't make it hard to add other objective kinds beside it later, but a general objective/rule engine is explicitly not being built yet (simple now, extensible later).

**Resource identity:** for now, a resource is just `{ device, AG-Linux path }` — no universal resource-ID abstraction. That gets introduced once a second OS or a non-filesystem resource type makes the need real, not before.

## Device / OS / Network architecture

This is a load-bearing decision for everything gameplay builds on top of, so it's recorded here even though the mechanics are in `ARCHITECTURE.md`:

- **AG Linux is the first implemented OS, not the universal abstraction for every computer in the game.** The world eventually contains a *pool* of OS families (other Linux variants, Windows-like, macOS-like, specialized/router/embedded OS). A `Device` is assigned one OS from that pool.
- **Higher-level gameplay (contracts, economy, reputation) talks to a `Device` through a small OS-agnostic seam**, never to AG-Linux-specific types directly: service/network visibility, authentication, resource interaction, session capabilities/privileges, and security/detection state. The seam exposes gameplay-level answers ("can this session read resource X," "does this identity have administrative privileges") — never the underlying mechanism (Unix permission bits vs. something else entirely on a future OS).
- **`Network` is OS-blind.** It only knows device identity, reachability, and topology — never what OS a device runs. What's actually exposed (which services, what they do) is answered by asking that device's OS through the seam.
- **OS state is persistent and device-owned; a session is separate and connection-scoped.** A target's filesystem/users/processes exist independent of whether anyone's connected. A session (identity, cwd, env — today's `ExecutionContext`) attaches to whichever device the player is currently interacting with, and detaches on disconnect. This finishes something the project's very first design doc already sketched (local filesystem vs. connected-remote-node filesystem) — Phase 7 is what makes it real.
- **No device is special-cased.** The player's own machine is a normal `Device` like any other — same OS, same network behavior, same seam. Any practical advantage (owning it, having admin on it, having configured it) comes from ownership/context in game state, never from the `Device` object itself being exempt from anything.
- **A device's OS is fixed for its lifetime** for now — no reinstall/OS-swap mechanic. Could change later if a real gameplay reason shows up.

## Software economy

Money buys **convenience, capability, efficiency, information, and new approaches — not a bigger power number.** Five dimensions a tool can improve, not all of which every tool needs to touch:

- **Automation** — fewer manual steps
- **Speed** — less in-game time cost
- **Information depth** — e.g. a cheap scanner shows open ports; an expensive one also flags likely-vulnerable versions
- **Stealth** — cheaper tools tend to be noisier / more detectable
- **Reach** — cheap tools may simply fail against modern/hardened targets, or conversely a cheap niche tool might be the *only* thing that works against some legacy/embedded device type

The tradeoff: cheap/open-source tools stay manual, slower, and knowledge-dependent — but genuinely viable. A skilled player's own knowledge should sometimes match what another player buys. This means contract/target design has a real content-authoring obligation: the manual path has to actually work, not just be theoretically possible.

**Repositories** are bought separately from the packages inside them — buying repo access doesn't unlock everything in it. Packages within a repo solve different problems or change workflows; they are not simply "stronger than the previous tier." This maps directly onto AGPKG's existing `PackageManifest`/`Repository`/`InstalledDatabase` model (Phase 6) — no new data shape needed, just content.

**Progression tier:** Open Source → Commercial/Proprietary → Specialized/Private → Restricted → Underground/Black-market. This is about access to different software *markets*, not RPG rarity colors. **Underground-tier mechanics and detection mechanics are both intentionally undesigned right now** — flagged as real systemic features to build later (reputation-gated sellers, scarcity, unreliable/risky software, logs/traces/alerts/target-hardening as consequences), not flavor. The rest of the design should leave room for them rather than assume they don't exist.

A concrete tie-in worth remembering: AGPKG already tracks installed *version* separately from catalog version (Phase 6's `upgrade` logic). An outdated installed version is already most of a vulnerability model — a target running an old package version can be the exploitable surface, without inventing a parallel system.

## Careers and specialization

Multiple job categories can coexist without rigid classes: hacking/security, programming, system administration, networking, data/automation, forensics, others. A player can mix freely — no `Class: Hacker` selection. Specialization should emerge from what the player owns, has access to, and has learned, not from a chosen archetype.

**Progression currencies:** money, reputation, software owned, repository/network access, player knowledge, possibly hardware later. **Explicitly not** a traditional RPG stat sheet (no `Hacking: 27` / `Programming: 14`). Small numeric systems are fine if they serve a real gameplay purpose, but not as a skill sheet.

**Programming contracts are simulated/abstracted for now** — no literal code execution, no IDE/interpreter. But also explicitly not "own Python → auto-complete Python jobs." The intended texture is **lightweight technical decision-making**: choose a language, choose an approach/tool, work within constraints (time, compatibility, performance, complexity, available libraries, quality requirements), occasionally branch on a decision point. Owning a better tool should open an option (automate part of the task, handle a harder requirement) rather than solve the contract outright. Not a minigame — kept light.

## Slice 1: the first vertical slice

Goal: prove the whole loop works end to end — contract → device → network → OS seam → resolution → reward — without demonstrating every future hacking mechanic.

```text
Player Device
      │
   AG Linux
      │
   Network
      │
Target Device
      │
   AG Linux
      │
 target resource
```

Flow:

```text
Accept contract
    ↓
Receive credentials (handed to the player as part of the contract — no credential-discovery mechanic yet)
    ↓
connect <target> <username> <password>
    ↓
Remote session (proof-of-access — the resource is read on the target, not copied back; no scp-equivalent yet)
    ↓
Read required resource
    ↓
Objective satisfied
    ↓
disconnect
    ↓
Contract resolved
    ↓
Reward (economy balance increases)
```

Decisions locked for Slice 1 specifically (later slices can revisit any of these):

- **Proof-of-access, not exfiltration.** "Obtain the resource" means successfully reading it on the target while authenticated — not transferring it back to the player's own device. Exfiltration as a distinct mechanic is future content once the loop is proven.
- **Credentials are handed to the player up front.** The contract is "legitimate maintenance access," not a hack — this keeps Slice 1 about proving the loop, not about building the first exploit/credential-discovery system.
- **`connect` takes the password as a command argument, not an interactive prompt.** Consistent with the existing, already-documented reason `sudo`/`su` aren't wired into the shell yet — no UI loop exists for hidden input.
- **One shell session, not multiple.** `connect`/`disconnect` swap which `Device` the player's single active session is attached to (local ⇄ remote), matching the project's original pre-Phase-1 design ("active filesystem is either local or the connected remote node"). Multiple simultaneous sessions/panels is a plausible Godot-era feature, not needed now.
- **One hardcoded objective kind (`ObtainResource`)**, one target device, no constraints yet. Reward is a simple economy balance increase — there is currently no economy/wallet system at all; Slice 1 introduces the minimal one.

**New systems this requires** (vs. what Phases 1–6 already provide): `Device`, `Network` (identity registry + reachability, no ports/routing/segments yet), a `Contract`/`ObtainResource` type, a minimal economy balance, a `connect`/`disconnect`-driven session that's decoupled from a device's persistent OS state (splitting today's `Shell` struct, which currently conflates OS state and session), and `connect`/`disconnect` builtins.

**Status:** Slice 1 is playable end to end via `cargo run`, in a real full-screen terminal UI (Ratatui + Crossterm, alternate screen + raw mode — `cargo run` takes over the terminal like any other TUI app, not a plain stdin/stdout REPL). `shell::scenario::tutorial()` boots the player's machine plus `corp-fs01` (holding the contracted resource) with one `ObtainResource` contract already accepted; `connect corp-fs01 guest guest` then `cat /home/guest/report.pdf` resolves the contract and shows the new balance, with command history (up/down), line editing, and scrollback all working. Still missing: content — this is still just one hardcoded tutorial scenario, not a real contract board with more than one job.
