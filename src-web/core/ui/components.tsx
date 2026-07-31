// Shared UI primitives used across feature modules (AGENTS.md §4 core/ui).

import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from "react"

/** A shared button; `variant` drives the bloom accent treatment. */
export function Button({
  variant,
  className = "",
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "danger" | "ghost" }) {
  const variantClass =
    variant === "primary" ? "btn-primary" : variant === "danger" ? "btn-danger" : variant === "ghost" ? "btn-ghost" : ""
  return (
    <button className={`btn ${variantClass} ${className}`} {...rest}>
      {children}
    </button>
  )
}

export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input className="input" {...props} />
}

export function Textarea(props: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className="textarea" {...props} />
}

export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select className="select" {...props} />
}

export function Card({ title, children, className = "" }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <div className={`card ${className}`}>
      {title ? <h3 className="card-title">{title}</h3> : null}
      {children}
    </div>
  )
}

export function Badge({ children, tone }: { children: ReactNode; tone?: "accent" | "danger" }) {
  const toneClass = tone === "accent" ? "badge-accent" : tone === "danger" ? "badge-danger" : ""
  return <span className={`badge ${toneClass}`}>{children}</span>
}

/** Indeterminate loading indicator with the accent glow. */
export function Spinner({ label }: { label?: string }) {
  return (
    <span className="row">
      <span className="spinner" aria-hidden="true" />
      {label ? <span className="muted">{label}</span> : null}
    </span>
  )
}

/** Small helper for async text in a row (message + spinner combos). */
export function AsyncText({ pending, children }: { pending: boolean; children: ReactNode }) {
  if (pending) {
    return <Spinner />
  }
  return <>{children}</>
}
