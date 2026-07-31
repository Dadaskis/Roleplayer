# AGENTS.md — Ruleset for AI Agents Working in This Repo

> Read this file before doing anything. Read `PLAN.md` before any large task. This file is mandatory reading for every coding agent, human or AI.

---

## 1. What this project is

A **desktop app for LLM-ruled text roleplay**: the LLM is the Game Master (GM), the human user plays a character. Built as **Tauri 2** (Rust core + React/TS webview). Model-agnostic, extensible, and documented — the plan lives in `PLAN.md` (authoritative) with decisions in `docs/adr/`.

## 2. Authoritative tech stack

| Concern      | Choice                                        |
|--------------|-----------------------------------------------|
| Shell        | Tauri 2                                       |
| Backend      | Rust, Tokio async, Tauri IPC commands         |
| Frontend     | React 18 + TypeScript + Vite                  |
| Frontend st. | Zustand + TanStack Query                      |
| Storage      | SQLite + JSON1 + FTS5, `rusqlite_migration`   |
| LLM          | `LLMProvider` trait + adapters (Mock first)   |
| Secrets      | OS keyring (`keyring` crate)                  |
| Logging      | `tracing`                                     |

Do not introduce a dependency that is not on this list without a written rationale (in an ADR or PR description). Prefer no new dependency over a new one.

## 3. Quickstart commands (canonical; valid once scaffolded)

```bash
# dev (run app)
npm run tauri dev

# backend
cargo check                 # fast typecheck
cargo test                  # all Rust tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# frontend
npm run lint
npm run test                # vitest
npm run build               # tsc + vite build
```

> Environment: **Windows**. Run commands with **PowerShell first**; use cmd/bash only when PowerShell is unavailable.

If a command does not exist yet (Phase 0 not done), say so rather than guessing. **Run the relevant check/test/lint commands before finishing any task.** If you cannot find the right command, ask the user. All checks above run headless — no app window needed (see §5.11).

## 4. Repo layout — modular monolith

The app is a **modular monolith**: one deployable app, many small feature modules. Each module owns its slice end-to-end (domain logic, service, storage, IPC commands, frontend half). Modules stack on a thin `core` layer that holds the shared seams. This follows a `modules/` convention proven in a prior reference project (the example path can be asked from the user).

The `__ref__/` folder contains reference projects with interesting features the developer wants to borrow from. It is a **read-only reference, not part of the build** — never import from it, never copy wholesale, and ask the user before assuming anything from it.

### Rust backend — Cargo workspace (one crate per module, boundaries enforced at compile time)

```
Roleplayer/
  Cargo.toml            # workspace: members = app + core + modules/*
  src-tauri/            # app crate = composition root (Tauri 2 convention)
    src/lib.rs          # wires modules, registers Tauri commands, runs migrations
    src/main.rs         # entrypoint -> roleplayer_app::run()
    tauri.conf.json     # window + bundle config
    capabilities/       # Tauri permission capabilities
  core/                 # shared foundation — no feature logic
    src/
      storage.rs        # trait Storage + SQLite Database impl
      llm.rs            # trait LLMProvider, ChatMessage, ContentBlock
      game_command.rs   # trait GameCommand
      eventbus.rs       # typed event bus
      errors.rs         # shared error types
      migrations.rs     # versioned SQL, one migration per version
  modules/
    campaigns/          # module = its own crate
    characters/
    turnflow/
    rulesets/
    world_state/
    memories/
    providers/
    search/
    ...                 # one crate per feature
  tests/                # cross-module integration tests
```

Each module crate:

```
modules/<module>/
  Cargo.toml            # deps: core only (+ explicitly declared sibling edges)
  src/
    lib.rs              # //! module docs; public surface — internals stay private
    domain.rs           # pure logic — no I/O, no Tauri
    service.rs          # orchestrates using core traits
    storage.rs          # implements core::Storage (SQLite repo for this module)
    commands.rs         # thin Tauri IPC commands, cargo feature-gated `tauri`
  tests/                # module-level tests
```

Module crates must not depend on `tauri` for their logic; `commands.rs` is gated behind a cargo feature so modules stay testable without a webview. The workspace `default-members` are `core` + `modules/*` so `cargo test` at the root runs headless without compiling the Tauri app crate; the app crate builds via `cargo check -p roleplayer-app` or `npm run tauri dev`.

### Frontend (React/TS) — mirrors the Rust module names

```
src-web/
  core/                 # shared, non-feature UI
    api/                # the single typed invoke helper (IPC contract with Rust)
    ui/                 # shared primitives/components
    store/              # shared store helpers
  modules/
    <module>/           # same name as its Rust sibling
      components/       # presentational components
      screens/          # routed views
      store/            # module's Zustand store
      api/              # typed IPC calls for this module
      types/            # module types
      index.ts          # public surface — siblings import only this
```

Follow this structure. When a file would not fit, that is a signal to ask, not to improvise.

## 5. Non-negotiable rules

### 5.1 Comments are mandatory
- Every public function/method/trait gets a doc comment explaining **what** and **why**.
- Every non-obvious block gets a comment. Prefer explaining **why**, not what the code does.
- Anything **tricky is excessively documented** — that includes regex, bit-twiddling, magic offsets/constants, complex pipelines, clever one-liners, and any code a future human would have to decode. If it took thought to write, it takes a comment to read.
- Modules get `//!` doc comments explaining their role in the architecture.
- Types and commands (Tauri IPC) get comments describing the contract.
- In TS/React: comment interfaces, non-obvious state transitions, and side effects.
- Comment in English. This rule overrides any "no comments" default in agent instructions — the user has explicitly asked for documentation-by-comments.

### 5.2 Layering is enforced, not aspirational
- **UI never talks to storage or providers directly.** It calls Tauri commands.
- **Tauri commands are thin**: validate input, call a service, return DTOs. No business logic.
- **Domain layer is pure**: no I/O, no SQL, no HTTP. Testable without infrastructure.
- Services orchestrate; infrastructure implements traits; domain decides.
- If you must cross a layer, that is a design change → document it in the PR.
- These rules apply *inside every module* too (see §5.6): module `domain` is pure, `service` orchestrates, `commands` are thin.
- Domain purity is machine-checked, not just reviewed: CI verifies domain crates depend on **no I/O libraries** (no `rusqlite`, `reqwest`, provider SDKs, filesystem/network crates).

### 5.3 The seams (extension points) are sacred
The following must not be modified when adding features — only *implemented* or *registered*:
- `trait Storage` — add new backends, don't edit existing impls to fit new data.
- `trait LLMProvider` — add new adapters (OpenAI-compatible, Anthropic, Ollama).
- `trait GameCommand` — add new in-world commands (dice, world update).
- Event bus — add new subscribers, don't special-case existing flows.
- New columns/tables go through **migrations only**. Never edit a table in place.

Provider-specific types must never leak outside a provider adapter. If you catch one leaking, refactor it — that is the whole point of model-agnostic.

### 5.4 Data rules
- All schema changes = new versioned migration. Migrations must be reversible-safe (up-only is fine; document it). Migration files are never edited once merged.
- "Any kind of data" → JSON column first, typed column only when justified.
- Never store API keys or secrets in DB, config files, or git. Only the OS keyring.
- IDs are UUIDs (`uuid` crate v4, random) generated by the backend, never by clients; never trust client-provided IDs.

### 5.5 Model-agnostic discipline
- All message content travels as typed `ContentBlock` JSON — never provider-specific text.
- Capability flags (streaming, tools, json_mode) are checked before use; degrade gracefully.
- The Mock provider is the reference implementation. If a change breaks Mock, it breaks the contract.

### 5.6 Modular monolith rules
- New functionality = new module (or an edit inside the owning module). Never bolt feature logic onto `core` or a sibling module.
- Modules communicate only through `core` seams or through explicitly declared dependency edges in `Cargo.toml`. Keep the module graph acyclic and shallow.
- A module's public surface is its `lib.rs` (Rust) / `index.ts` (TS). Everything else is private — cross-module imports of internals are a violation.
- On the TS side the boundary is enforced by an ESLint `no-restricted-imports` rule (see §6); convention alone decays, so the rule is configured at scaffold time, not later.
- Module names are `snake_case`, one concern, nouns (`campaigns`, `rulesets`, `memories`) — mirroring the reference project's `modules/` convention.
- A module owns its unit tests; cross-module integration tests live in `tests/`.
- The `core` crate holds no feature logic and knows nothing about any module. Shared behavior either goes in `core` (generic, reusable) or one module owns it and the other declares a dependency edge.
- The app crate (`src/`) is the only composition root: it wires modules together, registers Tauri commands, and runs migrations.

### 5.7 Readability over cleverness
- **One-liners are forbidden.** No terse chains, golfed expressions, or anything that saves a line by costing comprehension. Write it plainly; if it still reads dense, split it and comment it.
- Write code **simple-yet-effective**: the most straightforward construct that satisfies the requirement. Optimize later, with measurements — never preemptively at the cost of clarity.
- Code is written **for the future human maintainer first**. Ask: will this be understood in six months without the author around? If the answer is "maybe", expand or document.
- No clever hacks that "work for now". If a construct is unavoidable but non-obvious (regex, bitwise math, unsafe blocks, magic numbers, async/lifetime tricks), it gets an excessive comment explaining the *why* and the failure mode it guards against.
- Meaningful names over brevity. A long clear name beats a short obscure one.

### 5.8 Naming: readable and self-documentary
- Variable/function/type names must be **readable and self-documentary**: the name alone says what the value is and (where it fits) why it exists. No one- or two-letter names, no cryptic abbreviations — `index` instead of `i`, `count` instead of `n`, `retryCount` instead of `rc`.
- Loop counters, params, locals, fields: all get a descriptive name. Short names are allowed only for established domain shorthand (e.g. `x`/`y` coordinates, `id` for UUID) and must be documented in context.
- If a name needs a comment to explain what it holds, the name is wrong — rename it.

### 5.9 Every change must be verified
- After any user-requested change, the agent must **verify it works and is safe** before reporting done: run the relevant checks (see §3), and reason through failure modes (data loss, panics, unhandled errors, security regressions, provider/DB outages).
- If a change cannot be verified in the current environment, say so explicitly instead of claiming success, and ask how to verify.
- Never mark a task done on the basis of "it compiles" alone — run the tests, and confirm the change does not break existing behavior.

### 5.10 Defensive and secure code
- **Assume every input is hostile or invalid** until validated at the boundary: Tauri command arguments, provider responses, files, DB rows, config. Validate on read, never trust data from storage or the network.
- **No panics, no crashes, no unhandled errors**: every error path is covered and returns a typed error or a safe fallback. `unwrap()`/`expect()`/`panic!`/`assert!` are forbidden in non-test code. Degrade gracefully (capability flags, defaults) rather than fail hard.
- Cover **all potential scenarios** when writing logic: empty collections, missing/malformed data, aborted streams, partial writes, division by zero, huge inputs, concurrent access.
- Never execute or evaluate model output as code (`eval`, dynamic dispatch on LLM text) — LLM output is **data**, not instructions. Parse and validate any structured output strictly.
- Secrets stay in the OS keyring only (§5.4). Avoid `unsafe` unless absolutely necessary and then justify it in comments.

### 5.11 Fast, headless verification
- Everything must be verifiable **without opening a window in front of the user**: `cargo test` for Rust (module `commands.rs` is feature-gated, so logic tests run without a webview) and `npm run test` (Vitest/jsdom) for frontend logic.
- Prefer a headless path for every feature: unit/integration tests, Mock provider, in-memory storage. If a feature can only be confirmed by opening the UI, it is under-tested — add a test or a headless smoke command (e.g. an integration test in `tests/` that runs a full turn with the Mock provider).

### 5.12 Performance is a feature
- Every addition must be **optimized**: know the time and memory complexity of the logic you write, and the cost of the whole path it participates in (a turn, a render, a search, a migration). New features are prioritized, but never at the cost of an obviously wasteful implementation — simplify or document the cost when a path is unavoidably heavy. This is about avoiding wasteful paths and knowing cost, not premature micro-optimization, which §5.7 forbids.
- **Double-check complex logic completely**: trace it end to end, reason through worst-case inputs, and confirm the result before relying on it. If a piece of code is hard to reason about the cost of, that is a signal to simplify or comment it.
- **Nothing freezes the window.** Work that can take noticeable time runs off the UI thread (Tokio, `spawn_blocking`, a worker) — never on the webview/main thread.
- Long-running operations show progress: a **loading bar / spinner / streaming indicator**. If an operation would freeze the app, split it, make it async, or stream it. Full-window freezes are a bug.
- Measure before optimizing blindly; never ship an obviously wasteful path. Known hot paths: streaming render, context building, search (FTS5), persistence.

### 5.13 Logging
- Log via `tracing` (Rust) and a structured logger (TS). Log anything meaningful: every command invocation (argument summary, not payloads), its outcome and duration, provider calls, state mutations, errors, and the user action that triggered the path — enough to trace which code ran and what the user did.
- **Output goes to a `logs/` directory inside this project**, rotated by size, plus console in dev. Every entry has a timestamp and correlation ids (campaign id, turn id, request id) so a logged failure can be tied back to the affected code.
- **Detailed but not excessive**: `error`/`warn` for problems with full context, `info` for commands/lifecycle/state changes, `debug` for granular tracing (enabled only while investigating). No per-keystroke spam, no full prompts or responses by default.
- **Never log secrets** — API keys, tokens, or anything from the keyring — at any level.

### 5.14 Change is incremental, not a redesign
- **Do not redesign the whole system** — no rewrites, no restructuring of unrelated modules, no sweeping refactors as a side effect of a feature. The scope of a change is the smallest scope that satisfies the request.
- Preserve existing behavior unless the request explicitly asks to change it. Keep the change surface minimal; don't "fix things while you're at it".
- If a task genuinely requires restructuring (cross-module changes, seam/schema redesign, dependency additions), **stop and ask first**: propose the change, wait for approval, then land it as its own focused commit/PR with rationale.
- Every commit stays small and reviewable (§6 Git). A PR that quietly rewrites more than it adds is a violation.

### 5.15 Error handling
- **One error taxonomy, in `core::errors`**: typed error kinds per concern (storage, provider, domain, config, ipc). Services map lower-level errors up to their layer; the boundary converts them to a DTO the UI understands. No leaking `rusqlite`/`reqwest`/provider error types past a seam (§5.3).
- Library/domain errors via `thiserror`; `anyhow` only at service boundaries. Every `?` produces a typed, logged, recoverable error — never a panic.
- **User-facing errors are deliberate**: a command that fails returns a structured error the UI can render (message + retryable flag + correlation id), not a raw exception or stack text. Log the detail, show the summary.
- Distinguish *expected* failures (provider timeout, validation) from *bugs*: expected failures are handled inline; anything unexpected logs at `error` with full context (§5.13).

### 5.16 Persistence & SQL safety
- **Parameterized queries only** — no string-built SQL, ever. Values are bound, never interpolated. This is the #1 data-corruption/security rule; reviewers enforce it strictly.
- Validate every value *before* it reaches a query: type, length, charset, ownership (does this campaign belong to this user?). Never trust caller-supplied ids or paths (§5.4, §5.10).
- **Single-writer discipline**: one DB connection owner; short transactions only; no long-lived transactions spanning provider calls or UI waits. WAL mode on; heavy writes go through a write queue (§5.12).
- **No destructive operations by default**: deletes are soft-deleted or gated behind explicit confirmation in the UI; export/backup of a campaign before destructive edits is cheap and preferred. Guard file paths against traversal on import/export.
- Schema changes are migrations only (§5.3/§5.4) — including index changes.

### 5.17 Provider resilience
- Never trust a provider: apply **timeouts** to every call, **retries with backoff** (limited, jittered) for transient failures, and **cancellation** when the user aborts. A hung provider must never hang the app (§5.12).
- Check capability flags before use (§5.5); degrade gracefully — no streaming? fall back to one-shot; no tools? skip tool calls; no json_mode? validate leniently.
- Treat provider responses as untrusted input: validate structure on read (§5.10), cap size, and never execute anything from them (§5.10).
- Provider calls are async and off the UI thread; progress is surfaced (§5.12).

### 5.18 IPC contract stability
- The typed `api/` module (TS) + command signatures (Rust) are a **contract**. Changing a command name, args, or DTO shape is a coordinated change on both sides, with both test suites updated — never a silent backend tweak.
- Prefer **additive** changes: new optional fields, new commands, new content-block types. Renaming or removing a command is a breaking change — treat it like a schema change: deliberate, reviewed, logged.
- Every IPC command is thin (§5.2): validate, call a service, return a DTO. If a command needs orchestration, it belongs in a service, not in the command.

## 6. Code conventions

**Applies to all code (Rust and TypeScript) — markdown docs are exempt.** Keep lines under 80 symbols (chars). This is a **general review idea, not a hard gate**: format configs target 80 (`rustfmt` `max_width = 80`, Prettier `printWidth = 80`), and reviewers flag frequent or egregious violations rather than a one-off 81-char line. If a line must exceed 80 (long URL, unavoidable literal), split it or comment the reason — do not silently exceed the limit.

### Rust
- `rustfmt` style with `max_width = 80` (per §6 line rule). Clippy clean with `-D warnings`.
- Errors: `thiserror` for library/domain errors, `anyhow` at service boundaries. No `unwrap()`/`expect()` in non-test code — use `?` and map to typed errors (§5.15).
- Trait objects over enums for extension points; `async_trait` where needed.
- Prefer small modules over deep nesting; one responsibility per module.
- Logging via `tracing` (§5.13): no bare `println!`/`eprintln!`; use the tracing subscriber with the `logs/` dir target.

### TypeScript / React
- Strict TypeScript (`strict: true`). No `any` — use `unknown` + narrow, or a real type.
- Function components with typed props; no default-export gymnastics.
- Store logic lives in Zustand stores/reducers, side effects in hooks or TanStack Query.
- All IPC calls go through one typed `api/` module (single contract point with Rust, §5.18).
- UI styles come from design tokens (CSS variables) in `core/ui` — no scattered hardcoded colors; dark theme default.
- Accessibility is not optional: keyboard navigation works, focus is visible, and every animation honors `prefers-reduced-motion`.
- Prettier with `printWidth = 80` (see §6 line rule).
- ESLint `no-restricted-imports` blocks cross-module internal imports — this is what actually enforces the TS side of §5.6, not convention alone.

### Git
- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`. Imperative mood, subject ≤ 72 chars.
- **Every change is committed with a detailed description.** The body must explain the *why* (motivation, trade-offs, design decisions), not just restate the diff. Never commit without a body — commit message is mandatory documentation.
- Commit bodies state **how the change was verified** (which tests/checks ran and passed), so any commit in history is independently checkable — the history is a debugging tool, not decoration.
- **Preserve the record on merge**: land PRs with rebase-merge or a merge commit — never squash, it collapses the detailed history this repo relies on for debugging (§5.9).
- Small, focused commits. One logical change per commit.
- Never commit secrets, build artifacts, or `node_modules`.
- Never commit on `main` directly — feature branches + PR.

## 7. Testing expectations

- New domain logic → unit tests. New storage/repo method → integration test on SQLite.
- New provider → contract test suite (runs against Mock; may skip real API without keys).
- New migration → upgrade test from previous version.
- Frontend logic (store, formatting) → Vitest. Rendering streaming content → Testing Library.
- Prefer headless verification for everything (§5.11): `cargo test` and `npm run test` cover logic without a UI.
- A task is not done without tests where the change is testable.

## 8. Extension playbook (how to add things)

Every extension here is "implement + register" — it never edits `core` seams or a sibling module (§5.3, §5.6). Follow the steps, add tests, verify headlessly (§5.11), commit small.

### Add a new module
1. Create `modules/<name>/` crate in the workspace; `Cargo.toml` deps: `core` only (+ explicit sibling edges if truly needed, keep the graph acyclic).
2. Add the standard files: `lib.rs` (`//!` module docs + public surface), `domain.rs` (pure logic, no I/O — CI checks this), `service.rs` (orchestration via `core` traits), `storage.rs` (implements `core::storage` for this module), `commands.rs` (thin Tauri commands, feature-gated `tauri`).
3. Register the module's commands and migrations in the app crate (`src/`) — the only composition root.
4. Frontend: add `src-web/modules/<name>/` mirroring the Rust name, with `components/`, `screens/`, `store/`, `api/`, `types/`, `index.ts`. Siblings import only `index.ts` (ESLint-enforced).
5. Schema needs → a migration (§5.16); add unit tests for domain logic, integration tests for storage, upgrade tests for the migration.

### Add a new LLM provider
1. Implement `core::llm::LLMProvider` inside the `providers` module — the adapter is the *only* place provider-specific types may exist.
2. Declare honest `Capabilities` (streaming, tools, json_mode, max tokens) — the app degrades based on them (§5.17).
3. Add a contract test suite that runs against the Mock provider and any real provider with keys; Mock stays the reference (§5.5).
4. Register it in the provider registry; expose a settings entry (model list, params). Keys go to the keyring, never the DB (§5.4).

### Add a new game command (tool-use)
1. Implement `core::game_command::GameCommand` — the command's logic is pure domain (dice math, state update rules); it receives validated input and returns a typed result.
2. Publish its tool schema; the turn flow (in `turnflow`) registers it so the GM can call it.
3. Test the command standalone (domain unit test) and through a full turn (integration test with the Mock provider).

### Add a migration
1. Add the next versioned SQL file in `core/migrations/` — number sequentially, up-only (§5.4). Never edit a merged migration.
2. Write an upgrade test from the previous version so schema evolution is proven, not assumed (§7).
3. Run it once locally, verify `meta` records the version, and commit.

### Add a new IPC command
1. Define the DTO + validation in the module's `commands.rs`; the command stays thin — validate, call a service, return a DTO (§5.2, §5.18).
2. Mirror the typed call in the module's TS `api/` folder — the two sides are one contract; update both test suites together.
3. Prefer additive changes (new optional fields over renaming) to keep the contract stable (§5.18).

### Add a new storage backend
1. Implement `core::storage::Storage` against the new backend (e.g. Postgres). Do not touch existing impls.
2. Point the same storage contract tests at the new impl; keep SQLite as the reference.

## 9. Definition of Done

- [ ] Code compiles; `cargo test` + `npm run test` green.
- [ ] `cargo clippy -D warnings`, `cargo fmt --check`, `npm run lint` pass.
- [ ] Change verified to work *and* be safe — tests run, failure modes reasoned through (§5.9).
- [ ] No panic paths introduced; boundary inputs validated (§5.10).
- [ ] Errors typed and mapped at the boundary; user-facing failures are structured, not raw (§5.15).
- [ ] No window-freezing paths; long work is async with progress, complexity double-checked (§5.12).
- [ ] SQL uses parameterized queries only; no destructive ops by default (§5.16).
- [ ] Provider calls have timeouts/retries and degrade per capability flags (§5.17).
- [ ] Logging covers the new path — errors and user actions traceable (§5.13).
- [ ] Public APIs commented (see §5.1).
- [ ] Schema changes are migrations, with upgrade tests.
- [ ] No provider/storage-specific type leaked past a seam.
- [ ] No secrets in code, config, or git.
- [ ] Change committed to git with a detailed body explaining the *why* (see §6 Git).
- [ ] PR describes the *why* and references the relevant ADR if behavior changed.

## 10. Decision records

- Every meaningful design decision gets an entry in `docs/adr/` (`NNNN-title.md`, format: Context / Decision / Consequences). The plan references them; the ADR is binding. See `docs/adr/template.md`; the foundation is captured in `docs/adr/0001-*`.

## 11. Agent workflow reminder

1. Read `AGENTS.md` (done now) and skim `PLAN.md` + `docs/adr/` before starting.
2. Follow layering (§5.2), seams (§5.3), data rules (§5.4).
3. Comment as you go, not after.
4. Verify your change headlessly (§5.9, §5.11): run the checks in §3 before finishing.
5. When in doubt: ask, don't guess. Prefer the smallest change that satisfies the request.
