# Fatherload Agent Guidelines

Guidance for AI coding agents and contributors working in this repository.

## Product Direction

Fatherload is a Rust-native 2D mining game mechanically inspired by classic drill/mining games such as Motherload, with original names, assets, story, and presentation.

The game should be desktop-first for now, while keeping gameplay state and rules separate from rendering/input enough to avoid locking out future mobile, web, or WASM ports.

## Workspace Organization

- This repository is a Cargo workspace.
- Crates live under `packages/`.
- Add crates only when implementation needs them.
- Do not create speculative empty crates.
- Do not create generic `core`, `common`, `shared`, or similarly vague crates.
- Crates and modules should be domain-driven and named for the capability they own.
- Use sibling `models/` crates only when shared serializable data types need to be depended on without pulling in implementation behavior.

## Rust Conventions

- Use Rust edition 2024.
- Centralize dependency versions in the root workspace `Cargo.toml`.
- Package `Cargo.toml` files should use `workspace = true` for dependencies.
- Prefer `BTreeMap`/`BTreeSet` over `HashMap`/`HashSet` unless unordered hashing is required and justified.
- Keep gameplay logic decoupled from rendering/input adapters where practical.
- Placeholder programmer art is acceptable early, but avoid baking placeholder assumptions into domain logic.

## Lints

Rust crates should enable strict lints:

```rust
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
```

Package crates should expose a `fail-on-warnings = []` feature.

## Validation

After code changes, run these commands before finishing when practical:

1. `cargo fmt`
2. `cargo check --workspace`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. Relevant `cargo test` commands once tests exist

If any required command cannot be run, explain why in the final response.

## Completion Reporting

Final responses for coding tasks should summarize what changed and report validation exactly, including pass/fail/skipped status for each command run.
