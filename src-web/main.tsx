// Frontend entrypoint: mounts the React tree with the query client and theme.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import React from "react"
import ReactDOM from "react-dom/client"

import { App } from "./App"
import "./core/ui/theme.css"

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Data is cheap to refetch; keep it fresh without stale churn.
      staleTime: 5_000,
      retry: 1,
    },
  },
})

const root = document.getElementById("root")
if (!root) {
  throw new Error("missing #root element")
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
)
