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

**Objective types** will eventually include `DeleteResource`, `ExecuteAction`, `GainAccess`, etc. — a real extensible objective system, later. Slice 1 hardcodes two: `ObtainResource` ("authenticate to the target and copy the specified resource home — see `download` below) and `ModifyResource` (authenticate and overwrite the specified resource with required content, e.g. `echo ... > path` while connected). `career::contract::Objective` is the enum; adding a third kind means adding a variant plus one `ContractBoard::record_*` resolver beside `record_download`/`record_modify`, not touching `Contract`'s other fields or how contracts are stored. A general objective/rule engine is still explicitly not being built (simple now, extensible later).

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
Remote session (authenticated on the target)
    ↓
download required resource (copies it home; `cat` alone no longer satisfies the objective)
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

- **Exfiltration, not mere proof-of-access.** "Obtain the resource" means actually copying it back to the player's own device (`download <remote-path> [local-path]`, which reads it off the target and writes it into the local device) — a `cat` on the target no longer satisfies a contract by itself. `download` is the one scp-equivalent for now, and it follows `cp`/`scp`'s own destination rule: the optional `local-path` resolves against the *local* session's cwd (not wherever `connect` currently has you); if it names an *existing* local directory, the file lands inside it under the remote file's basename, otherwise it's used as the exact destination filename (a non-existent path is never auto-created as a directory — `mkdir` it first if that's what's wanted); with no `local-path` given at all it lands under local `$HOME` by the remote file's basename. No recursive/directory transfer.
- **Credentials are handed to the player up front.** The contract is "legitimate maintenance access," not a hack — this keeps Slice 1 about proving the loop, not about building the first exploit/credential-discovery system.
- **`connect` takes the password as a command argument, not an interactive prompt.** Consistent with the existing, already-documented reason `sudo`/`su` aren't wired into the shell yet — no UI loop exists for hidden input.
- **One shell session, not multiple.** `connect`/`disconnect` swap which `Device` the player's single active session is attached to (local ⇄ remote), matching the project's original pre-Phase-1 design ("active filesystem is either local or the connected remote node"). Multiple simultaneous sessions/panels is a plausible Godot-era feature, not needed now.
- **Two hardcoded objective kinds (`ObtainResource`, `ModifyResource`)**, one target device per contract, no constraints yet. Reward is a simple economy balance increase — there is currently no economy/wallet system at all; Slice 1 introduces the minimal one.

**New systems this requires** (vs. what Phases 1–6 already provide): `Device`, `Network` (identity registry + reachability, no ports/routing/segments yet), a `Contract`/`Objective` type, a minimal economy balance, a `connect`/`disconnect`-driven session that's decoupled from a device's persistent OS state (splitting today's `Shell` struct, which currently conflates OS state and session), and `connect`/`disconnect` builtins.

**Status:** Slice 1 is playable end to end via `cargo run`, in a real full-screen terminal UI (Ratatui + Crossterm, alternate screen + raw mode — `cargo run` takes over the terminal like any other TUI app, not a plain stdin/stdout REPL). `shell::scenario::tutorial()` boots the player's machine plus four target devices, one per job, with every contract posted to the board as `Available` — nothing pre-accepted. `contracts` lists available/active/completed jobs, including each one's login; `contracts accept <id>` takes one; `connect <host> <user> <password>` then either `download <resource> [local-path]` (three `ObtainResource` jobs — copies the resource home and pays out via `ContractBoard::record_download`) or an in-place overwrite like `echo ... > <resource>` (one `ModifyResource` job — pays out via `ContractBoard::record_modify` once the new content matches). `cat`ing a resource on its own never resolves anything. Each target has its own distinct account (`UserDatabase::add_account`, alongside the still-present default `guest`/`guest`) rather than every job reusing the same credential. Command history, line editing, mouse-wheel scrolling, and Scroll Mode (`Ctrl+Space`) all working. Still missing: objective types beyond obtain/modify (e.g. delete a resource, gain privileged access) — and, more importantly, everything Slice 2 (below) exists to fix: Slice 1's contracts hand the player the entire solution (hostname *and* credentials) up front, so there is no investigation, only execution.

## Slice 2: investigation and discovery

Slice 1 proved the technical loop (contract → device → network → OS seam → resolution → reward). It didn't prove the game is a *hacking* game — the player never investigates anything, because every contract already names the target and hands over its password. Slice 2's goal is narrower than it sounds: prove that a contract can describe an **outcome** ("obtain this org's customer database") without describing the **path** to it, and that the player can recover that path using systems that already exist (`Network`, `UserDatabase`, `ServiceRegistry`, `PackageManager`) plus exactly two new ones (`Organization`, `Knowledge`).

```text
Organization ("Meridian Analytics")
      │
      ├── meridian-web01  (seed — named by whois; only the default guest/guest
      │                    account exists here, and a note in its home
      │                    directory leaks a credential)
      └── meridian-db01   (holds the actual resource, under a home directory
                           guest/guest cannot read; reachable only once the
                           leaked credential is reused here)
```

Flow:

```text
Accept a Guided contract ("obtain the customer database" + a coarse hint,
no hostname, no credentials)
    ↓
whois <organization>              — names every device the org owns
    ↓
scan <device>                     — reachability, then (with nmap owned)
                                     exposed services + versions
    ↓
connect to the seed device with the universal default guest/guest account
    ↓
cat the note left in its home directory — Knowledge records the credential
it leaks
    ↓
Reuse that credential against the sibling device in the same Organization
    ↓
Access granted (the reused password is an authored fact — the same
account/password was placed on both devices when the scenario was
written, never rolled at runtime)
    ↓
download the required resource
    ↓
Objective satisfied → Contract resolved → Reward
```

Decisions locked for Slice 2:

- **`Organization` is a first-class `world` type, not scenario-only data.** It holds an identity, public/flavor information, and the set of hostnames it owns — `Network` remains the single source of truth for the `Device`s themselves; `Organization` is a grouping/ownership layer on top, not a duplicate registry. This is deliberately built as real `world` state (not kept inside `shell::scenario` the way Slice 1's `Job`/`Task` are) because contracts referencing it, `Network` containing its devices, and `Knowledge` remembering the relationship are all real gameplay needs *now*, not hypothetical ones — and because reputation, multiple offices, employees, and other org metadata are foreseeable follow-ups that would otherwise force promoting scenario-private data into a world entity later. One `Organization` with 2–3 `Device`s is enough for Slice 2; the type itself should not grow beyond identity + public info + owned devices until a real need shows up.
- **Contracts gain a notion of *lead strength*, independent of `Objective`.** Slice 1's contracts always name a concrete `target_hostname` up front (a **Directed** lead). Slice 2 adds a **Guided** lead: the contract instead names an `Organization` plus a coarse hint, and `target_hostname` is only known once the player's `Knowledge` resolves the org to an actual device they've found. `ObtainResource`/`ModifyResource` resolution (`record_download`/`record_modify`) is unchanged — it still keys on a concrete `(hostname, path)`, which now just isn't knowable at contract-post time for a Guided contract. An **Open-ended** lead (org + weak/no hint, more than one device could satisfy the objective) is a plausible Slice 3 extension of the same mechanism, not part of Slice 2.
- **Exactly one weakness, exactly one hop: password reuse.** The seed device and the device that actually holds the resource share one account's password — placed there deliberately when the scenario is authored, the same way Slice 1's job content (file contents, credentials) is authored in `shell::scenario`, never generated at runtime. This is intentionally the smallest possible "investigation" — no privilege escalation, no config-file credential leak, no multi-step weakness chain. Those are real and worth doing (a filesystem/config-leak chain in particular), but each adds a system Slice 2 doesn't need to prove the core loop, so they're deferred.
- **`scan` reveals information, not verdicts.** A bare `scan` (no recon tool owned) returns only reachability — today's `Network::is_reachable`, unchanged. Owning `nmap` (already an inert `Repository` entry since Phase 6) upgrades `scan`'s output to exposed services with name and version — still just data the player has to interpret, never a message telling them what to do with it. A hypothetical higher tier can eventually add a flagged indicator like "possible outdated configuration" — still an observation, not a verdict ("VULNERABILITY FOUND — USE EXPLOIT X" is explicitly the wrong shape). The player, not the tool, connects a service version or a harvested credential to the action that actually works. This preserves the AGPKG tie-in from the Software economy section above (better tools improve information depth) without turning owned software into an automatic solver — a principle intended to hold for every future recon tier, not just Slice 2's.
- **`Knowledge` belongs to persistent player/career state, not to `Shell`.** `Shell` is session-scoped — it represents *where the player currently is* (`active_hostname`, `context`) and already happens to also hold `Economy`/`ContractBoard` today, which were introduced in Slice 1 without a dedicated persistent-state layer above `Shell`. `Knowledge` (discovered hostnames, org affiliations, service versions, harvested credentials) is what the player has *learned*, independent of where they're currently connected, and shouldn't disappear or reset with session concerns the way `context`/`active_hostname` conceptually could. The intended shape is a persistent layer above `Shell`:

  ```text
  GameState
      └── Career               (persistent player/progress state)
            ├── Economy
            ├── Knowledge
            └── Contracts / progress
  ```

  `Shell` continues to be how the CLI reaches this state (the same way builtins reach `Device`/`Network` today via `active_device()`, never by owning them outright), and a future Godot client would reach the identical `Career` state through its own UI instead of a terminal. Slice 2 does not build save/load — the ownership model is what matters now; persistence to disk is a separate, later concern. `Economy`/`ContractBoard` stay on `Shell` for Slice 2 rather than also moving into `Career` — consistency with the diagram above is a reasonable future cleanup, not required to prove the investigation loop.

**New systems this required** (vs. what Slice 1 already provided): `world::Organization`/`OrganizationRegistry`; `career::Career` (holding `career::Knowledge`) as a field on `Shell` distinct from `Shell`'s own session state; `career::contract::Lead` (`Directed`/`Guided`) sitting beside `Objective`; `whois`/`scan`/`intel` builtins; a `version` field on `system::service::Service`; `world::device::CredentialLead` (an authored fact — "this exact path leaks this credential when read" — checked from `cat` via `Shell::note_credential_leads_at`); and wiring `nmap` ownership (via the existing `PackageManager`/`InstalledDatabase`) into `scan`'s actual output instead of it being inert.

**Explicitly not in Slice 2** (real, worth doing, deliberately deferred so this stayed as small as Slice 1 was): privilege escalation chains, a complex vulnerability taxonomy, runtime/random vulnerability generation, detection/"heat" and consequences, an underground/black-market tier, selling intel for money, full repository tiering, multiple OS implementations, a generic exploit abstraction, and a generalized objective/rule engine.

> **The player should solve authored problems by combining information and existing systems, not by waiting for a random vulnerability roll.** This is the principle every future recon tool, weakness, and objective kind should be checked against — it's what keeps the game an investigation/sandbox instead of an RPG skill check dressed up as a terminal.

**Status:** implemented and playable end to end via `cargo run`. `shell::scenario::tutorial()` registers one `Organization` ("Meridian Analytics") owning two devices: `meridian-web01` (only the default `guest`/`guest` account; a `CredentialLead` on `/home/guest/todo.txt` leaks the `analyst`/`M3ridian2024` credential once that file is `cat`ed) and `meridian-db01` (the `analyst` account, home directory `chmod 0700`, holding `/home/analyst/customers.csv` — the guest account cannot reach it). One Guided contract, "Obtain the customer database" ($7,500, `Objective::ObtainResource`), is posted alongside Slice 1's four Directed jobs; `contracts` displays its `Organization` and hint but never its real hostname (`Lead::Guided`), unlike the Directed jobs' hostname+login (`Lead::Directed`). `whois <organization>` (args joined with a space, so it doesn't need quoting) lists every hostname the org owns and records the org→hostname relationship into `Career::knowledge`; `scan <hostname>` always reports reachability and, once `nmap` is installed on whichever device the player is currently at, also lists services with name/version (still no verdict, no "vulnerable" flag); `intel` reviews everything `Knowledge` has accumulated so far (discovered hosts, discovered credentials). Resolution is unchanged from Slice 1 — `download` still just triggers `ContractBoard::record_download` against the contract's real `target_hostname`, which is knowable in practice only once the player has actually found it. All of this is additive: Slice 1's four Directed jobs, `cat`/`download`/`echo >` mechanics, and every prior test are untouched. Still missing (deliberately, see above): everything under "Explicitly not in Slice 2."

## Slice 3: the first privilege escalation chain

**Goal:** prove that access to a device and access to a protected resource are different things. A contract should be completable only after the player discovers an authored credential leak, moves from a low-privilege account to an account allowed to use `sudo`, and copies a root-owned file home. Keep the existing `ObtainResource` objective and payout rule: the contract pays for a successful `download`, not for merely becoming root or reading the file.

One new Directed contract is enough for this slice. Its briefing gives the target hostname and the initial `guest` login, plus a clue pointing toward a readable application configuration file. The same target holds a root-owned resource under `/root` (directory `0700`, file `0600`). The configuration file contains the password of that target's seeded `admin` account, which already belongs to the `sudo` group; the scenario changes its default password to a target-specific one with `UserDatabase::set_password`. Reading the config also records that credential in `Knowledge` through the existing `CredentialLead` mechanism. The clue and file contents should make the relationship discoverable without printing a command sequence in the contract briefing.

The planned authored target is `audit-vault01`. Its Directed contract names `/root/recovery.key` and points the player toward `/etc/backup-agent.conf`; the latter is readable by `guest` and contains the target-specific `admin` password. The key remains inaccessible to both `guest` and `admin` until a privileged `download`. This adds one job after the existing Slice 2 Guided job, preserving the earlier contract ids.

```text
Accept contract → connect as guest → find and cat readable config
    → Knowledge records admin credential → su to admin
    → sudo download protected resource → file lands on local device
    → active ObtainResource contract resolves and pays once
```

The player may also disconnect and reconnect directly as the discovered admin; that is a valid use of the same credential. Root access is still required for the protected file. The contract checks the outcome, not whether the player typed `su` specifically.

**Smallest implementation path:**

1. Expose the existing `system::privilege::su` and `sudo` rules against the *active device's* `UserDatabase` and `Sudoers`. `su <user> <password>` switches the current remote identity until another switch or `disconnect`. The player's `sudo` syntax is `sudo <builtin> [args...]`: when authentication is required, the terminal shows `[sudo] password for <current-user>:` and collects the caller's password in a separate, non-echoed buffer. The password is never part of the command line, history, scrollback, or error text. Enter authenticates and runs the pending command; Esc/Ctrl+C cancels it. This is interactive UI state, while password verification remains in `system::privilege::sudo`.
2. Run the command given to `sudo` through the existing builtin dispatch so `sudo download <path>` uses `Shell::download` and the normal contract resolution path. The temporary root identity is restored after that builtin, including failures. No nested shell syntax, pipelines, or redirection inside `sudo` in this slice. Reject session-switching commands (`connect`, `disconnect`, `su`, and nested `sudo`) while the temporary identity is active, so restoring the caller cannot leave an identity attached to the wrong device. The shell must reject a missing command and deny callers outside `Sudoers`; it must not treat a failed nested command as success.
3. Add the target device, readable config, protected resource, and Directed contract in `shell::scenario::tutorial()`. Use ordinary `VirtualFS` permissions for the barrier; keep the credential-lead metadata as an authored discovery hook, not as an alternate permission system.

**Acceptance checks:** a guest cannot read or download the protected file; a wrong password cannot switch identity; the admin still cannot download it without `sudo`; a non-sudoer cannot run the privileged command; the sudo password is requested after the command and never echoed or saved in history; cancellation runs nothing; `sudo` restores the caller's identity even when the command fails; and a successful privileged `download` writes the local copy and pays an accepted contract exactly once. Existing Slice 1 and Slice 2 contracts must remain completable.

**Boundary:** this slice does not add an exploit command, arbitrary code execution, version-based vulnerabilities, heat/detection, extra objective kinds, or a generalized weakness engine. The configuration leak and target-specific admin password are authored scenario facts; `VirtualFS`, `UserDatabase`, `Sudoers`, `Knowledge`, and `ContractBoard` continue to supply the actual rules.

**Status:** the `su`/`sudo` shell bridge, non-echoed sudo prompt, authored `audit-vault01` scenario, end-to-end contract test, and authentication cache are implemented locally.

### Sudo authentication cache

After successful authentication, later `sudo <command>` calls on the same active device and uid skip the password prompt for five minutes. **Every command without `sudo` continues to run as the original user.** Each sudo command receives root identity only while that command runs, and the caller's identity is restored afterward. A successful cached sudo call refreshes the timeout; failed authentication never creates or refreshes a cache entry. Successful `connect`, `disconnect`, and `su` calls invalidate the entry. The cache belongs to `Shell` session state, bound to hostname and uid; `ui::App` only decides whether to show the password prompt. Expiration tests force the cache deadline into the past rather than sleep.

The prompt remains the ordinary user's prompt throughout. This cache is authorization to avoid retyping the password for another `sudo` invocation, not a change to the shell's effective identity between commands.

## Slice 4: the first service foothold

This job makes a service version actionable without turning `scan` into an automatic vulnerability verdict. The player first identifies an organization's host, installs `nmap`, scans it, and uses a clue in the contract to investigate one running service. A successful attempt provides a remote shell as the service's unprivileged account; obtaining the protected resource still uses the ordinary filesystem and `download` rules. A stopped or patched service blocks the attempt. The player does not receive root simply because an exploit succeeded.

**First increment — persistent observations (implemented):** an `nmap`-enhanced `scan` records each observed service's hostname, name, version, and running/stopped state in `Career::knowledge`; `intel` displays those observations after the player moves between devices. A scan without `nmap` records only reachability. Repeated scans update an observation rather than duplicating it. This is historical player knowledge, not a live view: patching or stopping a service changes the device immediately, while a previous observation stays visible until rescanned. Later actions must check current device state as well as any observation they require.

**Foothold increment (implemented):** one device has an authored weakness bound to a specific service and version, with a fixed unprivileged entry uid. A CLI `exploit <hostname> <service>` attempt checks reachability, the authored weakness, the service's current running state and version, and the player's matching observation. On success it attaches the existing single session to that device as the entry user, clearing any sudo authentication cache. `disconnect` returns home. The target account has a locked password because this is access through the service, not password authentication. The contract names an outcome and gives a clue; it does not print the exploit command sequence. The successful attempt records an entry in the target's `LogBook`; detection consequences remain later work.

The authored target is `meridian-edge01`, a third Meridian Analytics host. Its running `nginx 1.18.0` service has one host-local weakness that yields the already-seeded `www-data` uid (33). A Guided obtain-resource contract points toward a protected web-operations file under `/var/www/ops`; that directory and file are readable by `www-data` but not by `guest`, and no password for `www-data` is supplied. Other devices running the same nginx version have no authored weakness. The weakness must be checked against the service's actual owner uid so authored data cannot silently grant a different identity.

**Acceptance:** the service version appears in `intel` only after an `nmap` scan; rescanning updates it; a host-only scan reveals no service version; a wrong host, wrong service, missing observation, stopped service, or changed version grants no shell; a successful attempt starts as the service account, cannot read root-owned files, and can complete one authored obtain-resource contract using its own readable resource. Existing contracts and ordinary `connect` authentication remain intact. No generic exploit framework, random vulnerability rolls, detection meter, or repository tiers in this slice.

**Status:** playable end to end in `shell::scenario::tutorial()`. The service-version change used in tests is an internal simulation hook; no player-facing patch command is included in this slice.
