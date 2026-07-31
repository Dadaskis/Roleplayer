// Public surface of the chat module.
// Siblings import only this file (ESLint no-restricted-imports, §5.6);
// anything not re-exported here is private to this module.
// Barrel: App pulls ChatScreen from here; store/api stay importable too.

// Typed IPC wrappers over the turnflow commands (send/cancel/list).
export * from "./api"
// Zustand store holding transcripts, drafts, streaming flags.
export * from "./store"
// The chat screen component (streaming transcript + composer).
export * from "./screens"
