// Public surface of the search module.
// Siblings import only this file (ESLint no-restricted-imports, §5.6);
// anything not re-exported here is private to this module.

// Re-export the search wire type (`SearchResult`).
export * from "./types"
// Re-export the typed IPC wrapper (`searchMessages`).
export * from "./api"
// Re-export the `SearchPanel` screen for FTS5 transcript search.
export * from "./screens"
