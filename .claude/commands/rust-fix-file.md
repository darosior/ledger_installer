---
description: Fix all issues in a single Rust file
allowed-tools: Read, Write, Bash(cargo:*)
---

# Fix Rust File: $ARGUMENTS

1. Read the file and `docs/rust-agent-guide.md`
2. Apply ALL fixes from the standards guide
3. Run `cargo clippy --all-targets -- -D warnings`
4. Run `cargo test`
5. Show diff of changes
