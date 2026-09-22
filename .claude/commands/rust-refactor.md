---
description: Refactor Rust code following strict production standards
allowed-tools: Read, Write, Bash(cargo:*), Bash(git:*)
---

# Rust Refactoring Task

Refactor the specified code: $ARGUMENTS

## Pre-Flight
1. Read `docs/rust-agent-guide.md` for full coding standards
2. Run `cargo clippy --all-targets -- -D warnings` to see current state
3. Identify all lint violations and anti-patterns

## Refactoring Process
1. Fix all clippy warnings (pedantic + nursery enabled)
2. Replace `.unwrap()` / `.expect()` with proper error handling
3. Add `#[must_use]` to pure functions
4. Add `#[non_exhaustive]` to public enums/structs
5. Document all public items with `# Errors` / `# Panics` / `# Safety`
6. Use newtype pattern for type safety where applicable

## Post-Flight
1. Run `cargo fmt`
2. Run `cargo clippy --all-targets --all-features -- -D warnings`
3. Run `cargo test`
4. Summarize changes made

Do NOT proceed if any check fails. Report the issue and stop.
