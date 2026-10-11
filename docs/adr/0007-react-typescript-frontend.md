# ADR-0007: React + TypeScript frontend with Rust/WASM modules

- **Status:** accepted
- **Date:** 2026-10-11

## Context
The Lounge must reproduce a very complex, Discord-like UI. D-151 selected Option A.

## Decision
React 19 + TypeScript + Vite. Rust → WASM for markdown, permissions, E2EE, theme validation, and image tools (doc 03 §3).

## Alternatives considered
Leptos/Dioxus (Rust UI): nearly 100% Rust, but immature for a UI of this size (virtualized lists, rich editor, accessibility, PWA tooling).

## Consequences
Two languages in the repo. Shared logic must live in Rust crates compiled to WASM, never duplicated in TS.
