---
description: Audit Rust codebase for anti-patterns and standards violations
allowed-tools: Read, Bash(cargo:*), Bash(grep:*), Bash(rg:*)
---

# Rust Code Audit

Audit the codebase (or specific path: $ARGUMENTS) for violations.

## Checks to Perform

### 1. Panic Points
Find all uses of:
- `.unwrap()`
- `.expect()`
- `panic!()`
- `todo!()`
- `unimplemented!()`
```bash
rg -n "\.unwrap\(\)|\.expect\(|panic!\(|todo!\(|unimplemented!\(" src/
```

### 2. Missing Documentation
Check for undocumented public items:
```bash
cargo doc --no-deps 2>&1 | grep -i "warning"
```

### 3. Clippy Violations
```bash
cargo clippy --all-targets --all-features -- -D warnings 2>&1
```

### 4. Unsafe Code
```bash
rg -n "unsafe" src/
```

## Output Format
Provide a structured report:
1. **Critical** — Must fix (panics in lib code, missing safety docs)
2. **High** — Should fix (clippy warnings, missing error docs)
3. **Medium** — Improve (style issues, complexity)
4. **Low** — Nice to have (additional derives, minor refactors)
