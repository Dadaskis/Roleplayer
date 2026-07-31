// Public surface of the campaigns module — siblings import only this file
// (enforced by ESLint no-restricted-imports, AGENTS.md §5.6).
// This barrel is the module's contract: everything a sibling may touch is
// re-exported here, and nothing not listed below is importable externally.

// Wire types (Campaign/NewCampaign/UpdateCampaign) shared with api/store.
export * from "./types"
// Typed IPC wrappers over the Rust campaign commands.
export * from "./api"
// The store: active campaign selection + the refresh-version tick.
export * from "./store"
// The list/create screen rendered by the App shell.
export * from "./screens"
