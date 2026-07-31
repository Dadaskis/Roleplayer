// Public surface of the characters module.
// Siblings import only this file (ESLint no-restricted-imports, §5.6);
// anything not re-exported here is private to this module.
// Barrel: the single import point for the workspace Characters tab.

// Wire types (Character/NewCharacter/UpdateCharacter).
export * from "./types"
// Typed IPC wrappers over the Rust character commands.
export * from "./api"
// The roster panel (list + create/edit/delete) shown in the workspace.
export * from "./screens"
