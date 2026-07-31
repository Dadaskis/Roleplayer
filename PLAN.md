# PLAN — LLM Text Roleplay Desktop App

> Status: **APPROVED — Phase 0 ready to start**. Stack and core direction are decided (see §10); remaining details are resolved while the MVP is built. This document is the single source of truth until an ADR supersedes a section.
> Note: section numbers may shift during planning — re-verify cross-references (e.g. in AGENTS.md) before relying on them.

---

## 1. The Concept

A desktop application for **text-based roleplay** where an LLM acts as the **Game Master (GM)** and the human user plays a character. The user narrates what their character does; the GM interprets the world, advances the story, and may update persistent game state (inventory, health, world events, NPCs, etc.).

The app is not a chat clone. It is a **game runtime**: it must own state, remember long histories, apply a ruleset/system prompt, and expose that state to the model in a controllable way.

### Core loop (single "turn")

1. User submits a character action (free text, possibly structured commands later).
2. App bundles context: system/ruleset prompt + world state summary + recent history.
3. LLM responds (streamed), optionally producing **tool calls** to mutate game state.
4. App applies state mutations, persists everything, renders the response.

---

## 2. Guiding Principles (non-negotiable)

1. **Model-agnostic.** No provider-specific types leak past the provider seam. Swapping a model must be a config change, never a code change.
2. **Scalable data.** Any kind of data must be storable without schema churn: use document-style (JSON) columns alongside typed columns, versioned by migrations. The storage backend is behind a trait so it can grow from SQLite to something heavier later.
3. **Extensible by design.** Every extension point (new provider, new storage, new game command) is a well-defined seam. Adding a feature must not require editing core files.
4. **Documented everywhere.** Comments are mandatory: on public APIs, on non-obvious logic, and *especially* on the "why" behind design decisions. The code is the documentation.
5. **Simple first.** Do not build for the biggest case on day one; build seams that let us get there later.
6. **Long-session first.** The app is designed for extremely long-running roleplays: context is budgeted, history is windowed, and durable facts live in storage — not in the model's memory.
7. **Hallucination resistance.** The GM is an *agentic* AI: it changes the world only through validated tool calls, its world view is re-injected from the single source of truth every turn, and every state change is auditable. The model's opinion is data; the database is the truth.

---

## 3. Tech Stack (decided)

| Layer          | Choice                              | Why it fits                                                                 |
|----------------|-------------------------------------|-----------------------------------------------------------------------------|
| Shell          | **Tauri 2** (Rust core + webview)   | Small binary, low RAM, native performance; Rust enforces data integrity     |
| Backend        | **Rust** (Tokio async, Tauri IPC)   | Type-safe data layer, strong guarantees for persistence & concurrency       |
| Frontend       | **React 18 + TypeScript + Vite**    | Fast to build, mature ecosystem, easy to extend the UI later                |
| UI styling     | **Tailwind CSS** (added at Phase 2) | Fast iteration; component lib (shadcn/ui) can slot in later                 |
| State (front)  | **Zustand** + TanStack Query        | Lightweight store; server-state handling for streaming turns                |
| Storage        | **SQLite** + JSON1 + FTS5           | Embedded, zero-ops, fast; JSON columns = store *any* data; FTS5 = search    |
| Storage seam   | `Storage` trait                     | Swap to Postgres/object store later without touching domain code            |
| LLM seam       | `LLMProvider` trait + adapters      | Mock (reference) + **OpenAI-compatible first**; Anthropic/Ollama later      |
| Secrets        | OS keyring (`keyring` crate)        | Never store API keys in DB/files in plaintext                               |
| Migrations     | `rusqlite_migration`                | Versioned, testable schema evolution                                        |
| Logging        | `tracing` (Rust), console + `logs/` dir | Debugging long async flows and provider calls; rotated, correlated, no secrets |
| Tests          | `cargo test`, Vitest + Testing Library | Unit-test domain, integration-test storage/provider seams                 |
| Module layout  | **Modular monolith**: Cargo workspace (1 crate/module) + TS module folders | Compile-time boundaries; mirrors the reference project's `modules/` pattern (example path on request) |

- **First real adapter: OpenAI-compatible** — it covers the planned providers (OpenCode Go, OpenRouter) and most local servers. Anthropic and Ollama follow later.
- The **provider selection menu** UI (models, params, key entry) is modeled on the OpenCode provider picker — a really nice menu; the user will provide OpenCode's source for reference.

### Alternatives considered

- **Electron** — pure JS end-to-end, fastest initial velocity, huge ecosystem. Rejected as default: heavy binary (~150 MB+), high RAM, no type-safe data layer. *Fallback if Tauri friction exceeds its value.*
- **Qt / GTK native** — fastest, lightest, but slow UI iteration and weak ecosystem for rich chat UI. Rejected.
- **Postgres from day one** — too heavy for a single-user desktop app. Kept behind the `Storage` trait instead.
- **Rust backend without Tauri** (own windowing) — unnecessary reinvention. Rejected.

### Environment check (already confirmed)

- `cargo` present at `~/.cargo/bin` ✓
- Node.js present ✓
- Windows 10/11 has WebView2 by default ✓ (Tauri 2 requirement)
- MSVC toolchain present ✓ (Tauri 2 Windows requirement)

---

## 4. Architecture

### 4.1 Layered design (horizontal view)

The app is a **modular monolith**: one deployable binary made of small self-contained feature modules. Two dimensions meet: vertical modules (feature slices) and horizontal layers (shared seams). The diagram below is the horizontal view; §4.4 shows the vertical file layout.

```
┌─────────────────────────────────────────────────────────────┐
│  UI LAYER (React/TS)                                        │
│  Screens · Components · Streaming render · Store (Zustand)  │
├─────────────────────────────────────────────────────────────┤
│  IPC boundary (Tauri commands — thin, no business logic)    │
├─────────────────────────────────────────────────────────────┤
│  APPLICATION LAYER (Rust services)                          │
│  CampaignService · SceneService · TurnService · ProviderSvc │
├─────────────────────────────────────────────────────────────┤
│  DOMAIN LAYER (Rust, pure logic, no I/O)                    │
│  Entities · ruleset engine · game-command registry          │
├─────────────────────────────────────────────────────────────┤
│  INFRASTRUCTURE LAYER (Rust)                                │
│  Storage (SQLite impl of Storage trait) · Provider adapters │
│  EventBus · Logging · Keyring                               │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Key seams (the "extension points")

| Seam            | Trait / contract                              | What you add to extend            |
|-----------------|-----------------------------------------------|-----------------------------------|
| Storage         | `trait Storage`                               | new backend impl (e.g. Postgres)  |
| LLM provider    | `trait LLMProvider`                           | new adapter (e.g. Mistral)        |
| Game command    | `trait GameCommand` (tool-use)                | new in-world action (roll, find)  |
| Event bus       | typed events, subscriber registry             | new side-effect (autosave, undo)  |
| Export/import   | serialize/deserialize of full campaign        | new format (later)                |

**Rule:** adding a provider, backend, or command must never require modifying existing implementations — only registering new ones. This is the concrete meaning of "extensible".

### 4.3 Turn flow (sequence)

```
UI ──post turn──▶ TurnService
                     │
                     ├─▶ CampaignService : load campaign + ruleset
                     ├─▶ MemoryService  : build context (world state + history window)
                     ├─▶ ProviderSvc    : stream completion via LLMProvider
                     ├─▶ EventBus ─▶ UI : token stream render
                     │
                     ├─▶ tool calls? ─▶ GameCommand registry ─▶ mutate domain
                     ├─▶ Storage       : persist messages + state
                     └─▶ EventBus ─▶ UI : turn complete
```

### 4.4 Modular monolith (the file layout)

Each feature is a module owning its full slice — domain, service, storage, IPC commands, and its frontend half. Modules sit on a thin `core` crate that holds only the shared seams. This follows a `modules/` convention proven in a prior reference project (the example path can be asked from the user) and gives every feature a home, so nothing gets bolted onto a "misc" pile.

**Rust** — Cargo workspace, one crate per module, boundaries enforced at compile time:

```
Roleplayer/
  Cargo.toml            # workspace: members = app + core + modules/*
  src/                  # app crate = composition root
    lib.rs / main.rs    # wires modules, registers Tauri commands, runs migrations
  core/                 # shared foundation — no feature logic
    src/
      storage.rs        # trait Storage
      llm.rs            # trait LLMProvider, ChatMessage, ContentBlock
      game_command.rs   # trait GameCommand
      eventbus.rs       # typed event bus
      errors.rs         # shared error types
      migrations/       # versioned SQL, one file per version
  modules/
    campaigns/          # module = its own crate
      src/
        lib.rs          # //! module docs; public surface — internals stay private
        domain.rs       # pure logic — no I/O, no Tauri
        service.rs      # orchestrates using core traits
        storage.rs      # implements core::Storage for this module
        commands.rs     # thin Tauri IPC commands, cargo feature-gated `tauri`
      tests/            # module-level tests
    characters/         # ...
    turnflow/           # the GM/user turn orchestration
    rulesets/           # system prompts / house rules
    world_state/        # persistent world facts + variables
    memories/           # long-term fact extraction
    providers/          # LLM adapter registration + settings
    search/             # transcript/world-state search (FTS5)
  tests/                # cross-module integration tests
```

**Frontend** — React/TS folders that mirror the Rust module names:

```
src-web/
  core/                 # api/ (typed IPC helper) · ui/ · store/
  modules/
    <module>/           # same name as its Rust sibling
      components/  screens/  store/  api/  types/  index.ts
```

Module discipline:
- New feature = new module; never append logic to `core` or a sibling.
- Modules talk only via `core` seams or explicit dependency edges in `Cargo.toml`. The module graph stays acyclic and shallow.
- Public surface = `lib.rs` / `index.ts`; cross-module imports of internals are violations.
- Module names: `snake_case`, one concern, nouns (`campaigns`, `memories`).
- Layering (§4.1) applies *inside* every module: `domain` pure, `service` orchestrates, `commands` thin.

Rationale: compile-time boundaries keep modules honest (folder-only "modular" decays into a mess), module-local tests are natural, and the single composition root preserves the "monolith" simplicity — one DB, one migration chain, one app.

Compile time is a known tuning target for a many-crate workspace: keep the module count sane, and tune the cargo `dev` profile (e.g. `split-debuginfo`, `codegen-units`) so incremental builds stay fast. Boundaries are the reason for crates; consolidation is only considered if compile pain outweighs the boundary value.

### 4.5 Visual design direction

Design idea (reference, not code): a personal doc-site prototype the user likes has the *feel* we want — **bloomy glow** and **living animations** — but its cyan neon is too loud. For Roleplayer: same bloom/animation DNA, neutralized to a **modern dark mode**. (The example prototype path can be asked from the user.)

Direction:
- **Palette** — neutral dark surfaces (zinc-scale: near-black base, elevated panels, subtle white borders), light neutral text, one calm **accent** for bloom (soft indigo/violet, e.g. `#a5b4fc`-family). No cyan, no rainbow text glow.
- **Bloom, kept** — soft radial glows on accent elements, glow around interactive hover states, luminous focus rings. Bloom goes on *targets* (buttons, badges, active items), not on all body text.
- **Animations, kept** — subtle scanline/ambient overlay, smooth hovers and transitions, pulsing glow on primary actions, image zoom + lightbox, smooth scrolling. All `prefers-reduced-motion`-safe.
- **Modern shapes** — rounded corners, layered elevation, glassy `backdrop-filter` panels, clean typography (system/Inter stack + monospace for content), refined responsive behavior.

Not decided yet: exact accent hue, how much bloom on the chat surface, theme toggle (dark default, maybe light later). Resolve at Phase 2 when the UI is built.

### 4.6 The GM is an agent (hallucination resistance & long sessions)

The GM is not a chat echo — it is a **tool-using agent** over a guarded world model. The core loop (§4.3) becomes: inject the true world state → let the GM think and call tools → apply only validated mutations → re-inject the updated state next turn.

Hallucination defenses:
- **State changes only through tool calls.** The GM never free-texts facts into the transcript and claims them true — it mutates `world_state`, `characters`, etc. through `GameCommand`s, and every call is validated before it touches storage (§5.16). Free text stays narrative.
- **Single source of truth, re-injected every turn.** World facts in the prompt are built from the database, never from what the model previously *said*. If it isn't in storage, the GM doesn't "remember" it.
- **Audit trail.** Every applied mutation writes a `state_changes` row (tool, args, before/after, timestamp, turn id) — a hallucination is detectable, revertible, and debuggable from the transcript + logs (§5.13).
- **Strict schemas.** Every tool has a published schema; args are parsed and validated against it. Malformed or nonsensical calls fail loudly with a typed error the GM can recover from.

Long-session support:
- **Durable facts over model memory**: everything the world depends on lives in SQLite; the model is a stateless interpreter between turns.
- **Context budgeting**: history windowing + `memories` summaries so a 10,000-turn game stays coherent (Phase 3), designed in from the start.
- **Capability-aware degradation** (§5.17): a provider without tool-use can only narrate; world edits fall back to manual, user-confirmed editing.

---

## 5. Data Model

### 5.1 Schema sketch (v1, subject to change during Phase 0)

| Table        | Purpose                                          | Notes                                   |
|--------------|--------------------------------------------------|-----------------------------------------|
| `campaigns`  | A roleplay session: title, ruleset id, settings  | root aggregate                          |
| `scenes`     | Chapters/locations within a campaign             | optional layering; keep flat if unused  |
| `characters` | Player + NPCs: stats, bio, JSON extra            | `extra` is JSON — store anything        |
| `messages`   | Turn transcript                                  | `role`, `content` (JSON content blocks) |
| `world_state`| Per-campaign document: world facts, variables    | JSON1 queryable                         |
| `state_changes`| Audit log of every applied world mutation    | tool, args, before/after, turn id — anti-hallucination (§4.6) |
| `memories`   | Long-term extracted facts (GM-curated)           | enables scaling beyond context window   |
| `rulesets`   | System prompts / house rules as reusable presets | the GM's "brain"                        |
| `provider_cfg`| Saved provider+model configs                     | keys **never** here — keyring only      |
| `meta`       | schema version, app settings                     |                                       |

**Content blocks in `messages.content`** are JSON objects, e.g.:
```json
{ "type": "text", "text": "You enter the tavern." }
{ "type": "dice", "expr": "2d6+1", "result": 9 }
{ "type": "tool_call", "tool": "update_world", "args": {...} }
```
Storing content as JSON keeps the schema stable while the model of data grows — this is the "any kind of data" requirement in practice.

`world_state`, `memories`, and `state_changes` are the anti-hallucination core: storage is the truth, the transcript is narrative, and every world edit is recorded (§4.6).

### 5.2 Evolution strategy

- **All** schema changes go through versioned migrations. Never mutate a table in place.
- "Any kind of data" first tries a JSON column; only promote to a typed column when the field becomes query-heavy or correctness-critical.
- Migrations run at app startup inside a transaction; tests cover upgrade from every version.

---

## 6. LLM Provider Abstraction

The single most important seam. Contract:

```rust
/// A chat message in a provider-agnostic shape.
struct ChatMessage {
    role: Role,               // system | user | assistant | tool
    content: Vec<ContentBlock>, // text | image_url | tool_call | tool_result
}

/// Capability flags a provider advertises (streaming, tools, json_mode, max_tokens, ...).
struct Capabilities { ... }

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    async fn complete(
        &self,
        request: CompletionRequest,
        on_token: Option<Box<dyn Fn(String) -> () + Send + Sync>>,
    ) -> Result<CompletionResponse>;
}
```

- Adapters: **Mock** (reference, for tests), **OpenAI-compatible** first (covers OpenCode Go and OpenRouter — the planned providers), **Anthropic** and **Ollama** later.
- Provider **contract tests** run against the Mock and any provider the user has keys for, so drift is caught automatically.
- Streaming flows from provider → Rust → Tauri event → React renderer.
- The **provider selection menu** (models, params, key entry) is modeled on the OpenCode provider picker; source reference to be provided by the user.
- The **agentic GM loop** (§4.6) sits on this seam: tool schemas published from `GameCommand`s, calls validated before apply, degraded gracefully when a provider lacks tool-use.

---

## 7. Roadmap / Milestones

Each phase ends with runnable, tested, documented code.

### Phase 0 — Foundations (scaffold)
- Tauri 2 app boots (Windows target confirmed working).
- Cargo workspace scaffold: `core` crate + `app` composition root + first module (`campaigns`).
- `Storage` trait + SQLite impl + migrations + `meta` table.
- Logging wired (`tracing`), app config + keyring init.
- Repo configs at scaffold: `rust-toolchain.toml` (toolchain pin), `.editorconfig` (tabs, 80-char guide), `rustfmt.toml` (`max_width = 80`), Prettier (`printWidth = 80`), ESLint `no-restricted-imports` for module boundaries.
- Minimal CI (GitHub Actions) on push/PR: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run lint`, `npm run test`, `npm run build` — plus a **domain-purity guard** that fails if any domain crate depends on I/O libraries (rusqlite, reqwest, provider SDKs, filesystem/network crates).
- **Accept:** app opens, writes schema, runs migration tests, `cargo test` green.

### Phase 1 — MVP chat loop
- CRUD campaigns + characters; **multiple independent campaigns (roleplays) from the start** — each owns its world state, characters, and transcript.
- Provider seam with Mock + one real adapter (OpenAI-compatible).
- Streaming chat view; turn persisted to `messages`.
- **Accept:** user creates a campaign, sends a message, receives a streamed GM reply, transcript survives restart.

### Phase 2 — GM behaviors
- Ruleset editor + system-prompt builder from `rulesets` + world-state injection.
- **Agentic GM loop (§4.6)**: game command registry (dice roll, world update, NPC sheet), tool-call validation, `state_changes` audit trail.
- Settings UI: provider + model selection (OpenCode-style picker), key entry (→ keyring).
- **Accept:** GM uses dice/state tools to change the world; every change lands in `state_changes`; switching models is a dropdown.

### Phase 3 — Memory & scale
- `memories` table + extraction heuristic (summary of old turns).
- History windowing: context budget respected, overflow → memory.
- Search over transcripts (FTS5) and world state.
- **Accept:** very long campaigns (thousands of turns) stay coherent; user can search past turns.

### Phase 4 — Extensibility surface
- Full tool-use protocol across real providers (Anthropic + OpenAI tool calls).
- Export/import of campaigns (JSON file); branching (fork campaign).
- **Accept:** plugin-style command registration documented with example.

### Phase 5 — Hardening & packaging
- Installers (MSI/NSIS), auto-update path, error reporting, e2e test pass.
- **Accept:** fresh-machine install runs; upgrade preserves data.

---

## 8. Testing Strategy

| Scope      | Tool                  | What                                                          |
|------------|-----------------------|---------------------------------------------------------------|
| Domain     | `cargo test`          | ruleset engine, game commands, memory heuristics (pure logic) |
| Storage    | `cargo test`          | repo CRUD against SQLite; migration upgrade tests             |
| Providers  | contract tests        | run against Mock + configured real provider                   |
| Services   | unit + integration    | turn flow with Mock provider, in-memory store                 |
| Frontend   | Vitest + Testing Lib  | rendering streaming content, store reducers                   |

No logic belongs in IPC commands; that keeps 95% of the app unit-testable without a UI.

---

## 9. Dev Workflow & Tooling

- **Commands** (canonical once scaffolded, see AGENTS.md): `npm run tauri dev`, `cargo test`, `cargo clippy --all-targets --all-features`, `cargo fmt --check`, `npm run lint`, `npm run test`.
- **Commits:** Conventional Commits; imperative mood; subject ≤ 72 chars. **Every change must be committed with a detailed body** explaining the *why* — never without a body.
- **Docs:** this plan + `docs/adr/` for decisions + AGENTS.md ruleset. Comments in code are required, not optional.
- **Branching:** small feature branches → PR review → main. CI runs from Phase 0 (see Phase 0).

---

## 10. Decisions (resolved at review)

1. **Tauri** — decided, no question.
2. **SQLite** — decided; the right store for this kind of local app, behind the `Storage` trait.
3. **Provider priority** — OpenAI-compatible first (covers the planned OpenCode Go and OpenRouter); Anthropic/Ollama later. Provider picker UI modeled on OpenCode's menu (source to be provided).
4. **Tool-use** — yes, preferred. The GM is an agentic AI (§4.6): tool-call-only world mutations, validation, audit trail.
5. **Plugin system** — in-repo command registry only; dynamic modding is not planned.
6. **Roleplays** — multiple independent campaigns from the start (Phase 1). *Branching/forking within a campaign* is a separate feature and stays deferred to Phase 4.
7. **Name** — Roleplayer.
8. **Module boundaries** — Cargo workspace, one crate per module; compile-time enforcement, with compile time as an explicit tuning target (§4.4).

Still open (resolved while the MVP is built, not blockers): exact accent hue / bloom level / theme toggle (§4.5), memory-extraction heuristic details (Phase 3), tool schemas for the v1 commands (Phase 2).

---

## 11. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Provider API drift (schema, tools) | model-agnostic promise breaks | contract tests + Mock; capability flags |
| Context window saturation in long games | game quality degrades | memory extraction + windowing (Phase 3); durable facts in storage (§4.6) |
| Model hallucinates state changes | game state corrupts | tool-call-only mutations (§4.6); `state_changes` audit + validation; single-source-of-truth re-injection |
| Very long sessions degrade coherence | late-game quality drops | context budgeting from day one; windowing + memories (Phase 3); model stays a stateless interpreter |
| SQLite single-writer lock | stalls on autosave | short transactions; async write queue |
| Tauri IPC overhead for large transcripts | UI lag | stream in chunks; load history lazily |
| Over-engineering the seam layer | slow delivery | seams only where listed in §4.2; YAGNI elsewhere |

---

## 12. What "done" means for this plan

The stack and core direction are decided (§10). Remaining details (accent hue, tool schemas, memory heuristics) are resolved while building the MVP. Phase 0 starts next. Any change to a decided item becomes an ADR and this plan is updated to match.
