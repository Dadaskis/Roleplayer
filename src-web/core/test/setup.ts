// Vitest global setup (headless, §5.11): registers jest-dom matchers so every
// component test can assert rendered DOM without importing them per file.
// Importing for side effect only — the package augments vitest's expect
// with DOM matchers (toBeInTheDocument, toHaveTextContent, ...). This runs
// once per test file via the vitest `setupFiles` config entry; no exports
// are needed from here, the augmentation is all the setup we want.
import "@testing-library/jest-dom/vitest"
