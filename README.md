# Roleplayer

A desktop app for **LLM-ruled text roleplay**: the LLM acts as the Game Master
(GM), the human user plays a character. The user narrates what their character
does; the GM interprets the world, advances the story, and changes the game
state through validated tool calls.

The app is a **game runtime**, not a chat clone: it owns state, remembers long
histories, applies a ruleset/system prompt, and exposes all of that to the model
in a controllable, auditable way.

**Status: MVP.** The whole plan lives in [PLAN.md](PLAN.md) (authoritative);
design decisions are recorded in [docs/adr](docs/adr/).

## The core loop (a single "turn")

1. User submits a character action (free text, optionally structured commands).
2. The app bundles context: system/ruleset prompt + world state summary +
   recent history.
3. The LLM responds (streamed) and may call **game commands** (tools) to mutate
   game state — dice rolls, world updates.
4. The app applies the validated mutations, persists everything, renders the
   response.

Because the GM is *agentic* but the world is authoritative, every state change
goes through typed tool calls, is audited (`state_changes` with before/after),
and the world is re-injected from storage each turn — the model's opinion is
data, the database is the truth.

## Features

- **Model-agnostic providers**: Mock (reference, works offline) + an
  OpenAI-compatible adapter (OpenCode Go, OpenRouter, local servers); more
  adapters plug into the same seam. Capability flags let the app degrade
  gracefully (no streaming → one-shot, no tools → narration only).
- **Streamed turns** with live typing and cancellation.
- **Campaigns, characters, rulesets** — everything is document-style JSON
  columns plus migrations, so new data shapes never need schema churn.
- **World state** with an audit trail of every GM mutation.
- **Memories** and **full-text search** (SQLite FTS5) over the history.
- **A full app UI** (React) plus a typed IPC contract in one place.
- Secrets stay in the OS keyring; keys are never stored in the DB.

## Tech stack

| Layer      | Choice                                             |
|------------|----------------------------------------------------|
| Shell      | Tauri 2 (Rust core + webview)                      |
| Backend    | Rust, Tokio async, Tauri IPC                       |
| Frontend   | React 18 + TypeScript + Vite                       |
| State      | Zustand + TanStack Query                           |
| Storage    | SQLite + JSON1 + FTS5 (`rusqlite_migration`)       |
| LLM seam   | `LLMProvider` trait + adapters (Mock is reference) |
| Secrets    | OS keyring (`keyring` crate)                       |
| Logging    | `tracing` → `logs/` (rotated, correlated, no keys) |

## Repo layout — a modular monolith

One deployable app, many small feature modules. Each module owns its slice
end-to-end (domain, service, storage, IPC commands, frontend half) and stacks on
a thin `core` layer holding only the shared seams
(`Storage`, `LLMProvider`, `GameCommand`, event bus, errors, migrations).

```
core/          shared foundation — no feature logic
modules/*/     one crate per feature (campaigns, characters, rulesets,
               world_state, turnflow, providers, memories, search)
src-tauri/     app crate = composition root (wires modules, registers
               commands, runs migrations)
src-web/       React frontend mirroring the module names
tests/         cross-module integration tests
```

The module layout and every rule for working in this repo are documented in
[AGENTS.md](AGENTS.md) — read it before touching anything.

## Getting started

Prerequisites: Node.js, npm, and a Rust toolchain (stable). First run:

```bash
npm install
```

### Run the app (dev)

```bash
npm run tauri dev
```

### Headless checks (no window needed)

```bash
# backend
cargo check                 # fast typecheck
cargo test                  # all Rust tests (47)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# frontend
npm run lint
npm run test                # vitest
npm run build               # tsc + vite build
```

Note: `cargo test` at the root runs `core` + all modules without compiling the
Tauri app crate, so every check above runs headless.

## Documentation

- [AGENTS.md](AGENTS.md) — the repo's ruleset: architecture, layering,
  comment density, verification, definition of done.
- [PLAN.md](PLAN.md) — the authoritative plan (approved; sections 4-6 cover
  app design, agentic GM, and the UI).
- [docs/adr](docs/adr/) — decision records; every meaningful design decision
  gets an entry.
