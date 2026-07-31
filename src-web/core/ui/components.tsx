// Shared UI primitives used across feature modules (AGENTS.md §4 core/ui).

// Spread native HTML attributes through so callers keep full DOM control
// (onClick, aria-*, type, disabled, ...) without extra prop plumbing here.
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from "react"

/** A shared button; `variant` drives the bloom accent treatment. */
export function Button({
  variant,
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "danger" | "ghost" }) {
  // Map a named variant to its theme.css class; no/unknown variant falls back
  // to the plain `btn` base style rather than failing.
  // Destructured out of `rest` so the variant never leaks onto the DOM node
  // as an invalid attribute; className merges caller styles after the variant.
  const variantClass =
    variant === "primary" ? "btn-primary" : variant === "danger" ? "btn-danger" : variant === "ghost" ? "btn-ghost" : ""
  return (
    // Base class is always present; variant and caller classes ride along.
    <button className={`btn ${variantClass} ${className}`} {...rest}>
      {children}
    </button>
  )
}

// Thin form controls: they exist so every form shares the theme.css classes
// instead of repeating className strings (and their typos) at call sites.
// Each one only hardcodes its class and forwards everything else to the DOM.
export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input className="input" {...props} />
}

export function Textarea(props: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className="textarea" {...props} />
}

export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select className="select" {...props} />
}

// Title is optional so a Card can frame arbitrary content; when absent the
// heading is simply omitted instead of rendering an empty one.
export function Card({ title, children, className = "" }: { title?: string; children: ReactNode; className?: string }) {
  return (
    // className merges after the base so callers can override card padding.
    <div className={`card ${className}`}>
      {/* Conditional heading keeps the DOM clean when there is no title. */}
      {title ? <h3 className="card-title">{title}</h3> : null}
      {children}
    </div>
  )
}

// Tone colors are semantic (accent = highlight, danger = destructive) and map
// to theme.css badge variants; default stays the muted neutral badge.
export function Badge({ children, tone }: { children: ReactNode; tone?: "accent" | "danger" }) {
  // Unknown/no tone yields "" so the badge keeps its neutral base style.
  const toneClass = tone === "accent" ? "badge-accent" : tone === "danger" ? "badge-danger" : ""
  return <span className={`badge ${toneClass}`}>{children}</span>
}

/** Indeterminate loading indicator with the accent glow. */
export function Spinner({ label }: { label?: string }) {
  return (
    // Row layout puts the spinner and its optional label side by side.
    <span className="row">
      {/* aria-hidden: the spinner is decorative; the label carries meaning. */}
      <span className="spinner" aria-hidden="true" />
      {/* Label is optional so bare spinner-only usages stay compact. */}
      {label ? <span className="muted">{label}</span> : null}
    </span>
  )
}

/** Small helper for async text in a row (message + spinner combos). */
export function AsyncText({ pending, children }: { pending: boolean; children: ReactNode }) {
  // While pending we swap the content for a spinner — the parent keeps its
  // layout slot so nothing jumps when the async value resolves.
  if (pending) {
    return <Spinner />
  }
  // Once settled, render the children unchanged (usually the resolved text).
  return <>{children}</>
}
