// Frontend entrypoint: mounts the React tree with the query client and theme.

// Pull in the query client so every screen's useQuery calls share one cache.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
// React's JSX runtime; StrictMode below relies on the default import.
import React from "react"
// The webview host gives us createRoot to mount into; we never use hydrate.
import ReactDOM from "react-dom/client"

// The app shell: navigation + top-level view composition.
import { App } from "./App"
// The pop-out debug window root, mounted only on the `#/debug/<id>` route.
import { DebugRoot } from "./debug"
// Importing the stylesheet for its side effect — every component below
// references these classes, so they must be loaded before first paint.
import "./core/ui/theme.css"

// One cache instance lives for the whole session; all modules share it.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Data is cheap to refetch; keep it fresh without stale churn.
      staleTime: 5_000,
      // One retry absorbs transient IPC hiccups; the backend already maps
      // permanent failures to structured errors, so more retries only hide
      // real problems behind latency.
      retry: 1,
    },
  },
})

// The webview always ships a #root (see index.html); if it is missing, fail
// loudly at startup — a silent no-op render would just show a blank window.
// Resolve the mount point the bundler's index.html always ships.
const root = document.getElementById("root")
if (!root) {
  // Failing loudly beats rendering nothing: a silent mount would present as
  // a blank window with no trace of why.
  throw new Error("missing #root element")
}

// The debug window loads `index.html#/debug/<id>`; a matching hash mounts the
// pop-out debug root instead of the main app. The id is URL-encoded
// defensively in case a future id format needs it.
const debugMatch = window.location.hash.match(/^#\/debug\/(.+)$/)
const rootView = debugMatch ? (
  <DebugRoot campaignId={decodeURIComponent(debugMatch[1])} />
) : (
  <App />
)

// StrictMode double-invokes effects in dev; the chat screen's event
// subscription must survive that mount → unmount → mount cycle.
ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {/* Provide the shared cache down the tree so every screen's useQuery
        calls resolve against the same store (retries/staleTime above). */}
    <QueryClientProvider client={queryClient}>{rootView}</QueryClientProvider>
  </React.StrictMode>,
)
