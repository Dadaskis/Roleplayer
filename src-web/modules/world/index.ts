// Public surface of the world-state module.
// Siblings import only this file (ESLint no-restricted-imports, §5.6);
// anything not re-exported here is private to this module.

// Re-export the wire types so consumers get the `StateChange` shape.
export * from "./types"
// Re-export the typed IPC wrappers (the module's side of the contract, §5.18).
export * from "./api"
// Re-export the `WorldPanel` screen used to view and edit the world doc.
export * from "./screens"
