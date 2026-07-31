// Public surface of the memories module.
// Siblings import only this file (ESLint no-restricted-imports, §5.6);
// anything not re-exported here is private to this module.

// Re-export the memory wire types (`Memory`, `NewMemory`, `SummarizeRequest`).
export * from "./types"
// Re-export the typed IPC wrappers for list/create/delete/summarize.
export * from "./api"
// Re-export the `MemoriesPanel` screen for GM-curated long-term facts.
export * from "./screens"
