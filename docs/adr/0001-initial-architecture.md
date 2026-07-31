# ADR-0001 — Initial architecture foundation

> Status: accepted
> Date: 2026-07-31

## Context

A new desktop app for LLM-ruled text roleplay: the LLM is the Game Master, the user plays a character. Requirements from the start: model-agnostic, flexible storage for "any kind of data", extensible by adding features, and heavily documented for future human maintainers. No code exists yet; the full direction is in `PLAN.md`.

## Decision

- **Shell:** Tauri 2 (Rust core + React/TS webview). Small binary, low RAM, type-safe data layer.
- **Backend:** Rust, Tokio async, thin Tauri IPC commands.
- **Frontend:** React 18 + TypeScript + Vite, Zustand + TanStack Query, dark modern UI (§4.5 of PLAN).
- **Storage:** SQLite + JSON1 + FTS5 behind a `Storage` trait, versioned migrations, JSON columns first for flexible data.
- **LLM:** `LLMProvider` trait with adapters (Mock first as reference contract, then OpenAI-compatible, Anthropic, Ollama); content travels as typed `ContentBlock` JSON.
- **Architecture:** modular monolith — Cargo workspace with one crate per feature module plus a thin `core` crate holding the shared seams (Storage, LLMProvider, GameCommand, event bus); a single composition root (`src/`) wires modules and registers commands/migrations. Frontend mirrors module names.
- **Secrets:** OS keyring only. **Logging:** `tracing` to project `logs/`, rotated, correlated.
- **Extension model:** "implement + register" — new providers/backends/commands never edit `core`.

## Consequences

- Positive: model-agnosticism is enforceable (provider types can't leak past a seam); feature additions are additive; the compiler (Rust) and ESLint (TS) enforce module boundaries; most logic is testable headlessly without a webview.
- Negative / trade-offs: Rust + workspace adds build complexity vs. a single crate or Electron; Tauri IPC requires a typed contract maintained on both sides; SQLite single-writer needs discipline (short transactions, write queue).
- Notes: revisit if Tauri friction exceeds its value (fallback: Electron, per PLAN §3); revisit SQLite if multi-user/concurrent write load ever appears — the `Storage` trait is the seam for a Postgres backend.

Supersedes the initial draft stack section of PLAN.md; PLAN.md remains authoritative for the roadmap.
