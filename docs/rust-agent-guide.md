# Rust Production Code Standards for AI Agents

## Core Philosophy

Write Rust code that is **correct first, clear second, and fast third**. Leverage the type system to make invalid states unrepresentable. Prefer compile-time guarantees over runtime checks.

---

## 1. Clippy Configuration

Configure lints in `Cargo.toml` and `clippy.toml`, not inline attributes.

### Cargo.toml Lint Configuration

```toml
[workspace.lints.rust]
unsafe_code = "deny"
missing_docs = "warn"
rust_2018_idioms = "warn"
trivial_casts = "warn"
trivial_numeric_casts = "warn"
unused_lifetimes = "warn"
unused_qualifications = "warn"
unsafe_op_in_unsafe_fn = "warn"
future_incompatible = "warn"
nonstandard_style = "warn"

[workspace.lints.clippy]
# Lint groups
pedantic = "warn"
nursery = "warn"
all = "warn"
correctness = "deny"
suspicious = "warn"
style = "warn"
complexity = "warn"
perf = "warn"
cargo = "deny"

# Panic prevention (see Critical Path exceptions below)
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
todo = "warn"
unimplemented = "warn"

# Code quality
dbg_macro = "warn"
print_stdout = "warn"
print_stderr = "warn"
missing_errors_doc = "warn"
missing_panics_doc = "warn"
missing_safety_doc = "warn"
undocumented_unsafe_blocks = "warn"
float_cmp = "warn"
lossy_float_literal = "warn"
mem_forget = "warn"
inefficient_to_string = "warn"

# Forbidden
exit = "forbid"
infinite_loop = "forbid"

# Allowed (with justification)
module_name_repetitions = "allow"  # Often acceptable for clarity
redundant_pub_crate = "allow"      # Can conflict with visibility rules

[lints]
workspace = true
```

### clippy.toml Configuration

```toml
avoid-breaking-exported-api = false
cognitive-complexity-threshold = 10
excessive-nesting-threshold = 4
too-many-arguments-threshold = 5
too-many-lines-threshold = 60
type-complexity-threshold = 200
single-char-binding-names-threshold = 4
max-fn-params-bools = 2
max-struct-bools = 2
warn-on-all-wildcard-imports = true
check-private-items = true
```

---

## 2. Workspace Structure

Use depth-1 workspace members. Each member crate can have its own internal structure.

```
coastline/
├── Cargo.toml              # Workspace root
├── clippy.toml
├── rustfmt.toml
├── deny.toml
├── siren/                  # Member crate
│   ├── Cargo.toml
│   └── src/
├── keel/                   # Member crate
│   ├── Cargo.toml
│   └── src/
├── brig/                   # Member crate
│   ├── Cargo.toml
│   └── src/
├── cove/                   # Member crate
│   ├── Cargo.toml
│   └── src/
└── tests/                  # Integration tests
```

### Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "siren",
    "keel",
    "brig",
    "cove",
]

[workspace.package]
edition = "2024"
rust-version = "1.83"
license = "PROPRIETARY"

[workspace.lints.rust]
# ... (as defined above)

[workspace.lints.clippy]
# ... (as defined above)

[workspace.dependencies]
# Shared dependencies with versions
tokio = { version = "1.0", features = ["full"] }
thiserror = "2.0"
```

### Member Crate Internal Structure

```
siren/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── error.rs
    ├── config.rs
    └── domain/
        ├── mod.rs
        └── ...
```

---

## 3. Error Handling & Panic Policy

### Critical Bitcoin Paths — MUST PANIC

**For any path involving bitcoin operations or potential loss/confiscation of funds, panic immediately.** Do not attempt recovery.

```rust
/// Calculates transaction fee. Panics on overflow to prevent fund loss.
///
/// # Panics
///
/// Panics if the fee calculation overflows, which would indicate
/// a critical error that could result in loss of funds.
#[must_use]
pub fn calculate_fee(inputs: u64, outputs: u64, fee_rate: u64) -> u64 {
    let size = inputs
        .checked_mul(INPUT_SIZE)
        .expect("critical: input size overflow — potential fund loss");

    let total = size
        .checked_add(outputs.checked_mul(OUTPUT_SIZE)
            .expect("critical: output size overflow — potential fund loss"))
        .expect("critical: total size overflow — potential fund loss");

    total
        .checked_mul(fee_rate)
        .expect("critical: fee calculation overflow — potential fund loss")
}

// For arithmetic in bitcoin paths, prefer checked operations that panic
impl Balance {
    pub fn debit(&mut self, amount: Satoshis) {
        self.0 = self.0
            .checked_sub(amount.0)
            .expect("critical: balance underflow — attempted overdraw");
    }
}
```

### AVOID `unwrap_or_default`

Be extremely cautious with default value setters. There are seldom cases where a default is acceptable when an unwrap fails.

```rust
// BAD: Silently uses 0, could cause incorrect calculations
let amount = parse_amount(input).unwrap_or_default();

// BAD: Empty string might propagate silently
let address = config.get("address").unwrap_or_default();

// GOOD: Explicit handling
let amount = parse_amount(input)
    .map_err(|e| ProcessError::InvalidAmount { input, source: e })?;

// GOOD: If default is truly acceptable, be explicit about why
let timeout = config.timeout_ms.unwrap_or(30_000); // 30s default is safe
```

### Socket/API Endpoints — Return Errors

External-facing endpoints should return usable errors, not panic.

```rust
/// API handler — returns error, does not panic.
pub async fn get_balance(
    Path(account_id): Path<AccountId>,
) -> Result<Json<BalanceResponse>, ApiError> {
    let account = accounts
        .get(&account_id)
        .ok_or(ApiError::NotFound { resource: "account", id: account_id })?;

    Ok(Json(BalanceResponse { balance: account.balance }))
}
```

### Custom Error Types

```rust
use thiserror::Error;

/// Errors from the processing module.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProcessError {
    /// Invalid input was provided.
    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },

    /// An I/O error occurred.
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    /// Operation timed out.
    #[error("timeout after {duration:?}")]
    Timeout { duration: std::time::Duration },
}
```

---

## 4. Type System — Use to Fullest Extent

Leverage Rust's type system for safety, clarity, and zero-cost abstractions.

### Newtypes for Type Safety

```rust
/// Validated bitcoin address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address(String);

impl Address {
    /// Creates a new address if valid.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        validate_address(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

/// Amount in satoshis — prevents unit confusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Satoshis(u64);
```

### Generics (Templates)

```rust
/// Generic repository pattern.
pub struct Repository<T, Id> {
    storage: HashMap<Id, T>,
}

impl<T, Id: Hash + Eq> Repository<T, Id> {
    pub fn get(&self, id: &Id) -> Option<&T> {
        self.storage.get(id)
    }

    pub fn insert(&mut self, id: Id, item: T) -> Option<T> {
        self.storage.insert(id, item)
    }
}
```

### Traits for Common Functionality

```rust
/// Common interface for all signers.
pub trait Signer: Send + Sync {
    fn sign(&self, message: &[u8]) -> Result<Signature, SignError>;
    fn public_key(&self) -> &PublicKey;
}

/// Extend with blanket implementations where useful.
pub trait SignerExt: Signer {
    fn sign_transaction(&self, tx: &Transaction) -> Result<SignedTx, SignError> {
        let sig = self.sign(&tx.sighash())?;
        Ok(SignedTx { tx: tx.clone(), sig })
    }
}

impl<T: Signer> SignerExt for T {}
```

### Static Dispatch (Default)

```rust
/// Prefer static dispatch for performance-critical code.
pub fn process<S: Signer>(signer: &S, data: &[u8]) -> Result<Output, Error> {
    let sig = signer.sign(data)?;
    // Compiler monomorphizes — zero runtime cost
    Ok(Output { sig })
}
```

### Dynamic Dispatch (When Needed)

```rust
/// Use `dyn` when you need runtime polymorphism or to reduce binary size.
pub struct SignerRegistry {
    signers: HashMap<String, Box<dyn Signer>>,
}

impl SignerRegistry {
    pub fn get(&self, id: &str) -> Option<&dyn Signer> {
        self.signers.get(id).map(|s| s.as_ref())
    }
}

/// Also useful for trait objects in async contexts.
pub type BoxedSigner = Box<dyn Signer + Send + Sync>;
```

### Type Erasure

```rust
/// Erase concrete types when implementation details shouldn't leak.
pub fn create_signer(config: &Config) -> impl Signer {
    match config.signer_type {
        SignerType::Hardware => HardwareSigner::new(config),
        SignerType::Software => SoftwareSigner::new(config),
    }
}

/// Or with explicit type erasure for collections.
pub fn load_signers(configs: &[Config]) -> Vec<Box<dyn Signer>> {
    configs.iter().map(|c| -> Box<dyn Signer> {
        match c.signer_type {
            SignerType::Hardware => Box::new(HardwareSigner::new(c)),
            SignerType::Software => Box::new(SoftwareSigner::new(c)),
        }
    }).collect()
}
```

### Make Invalid States Unrepresentable

```rust
// BAD: Can have invalid states
struct Transaction {
    signed: bool,
    signature: Option<Signature>,
}

// GOOD: Type system prevents invalid states
enum Transaction {
    Unsigned(UnsignedTx),
    Signed { tx: UnsignedTx, sig: Signature },
    Broadcast { tx: UnsignedTx, sig: Signature, txid: Txid },
}
```

---

## 5. Concurrency — Prefer Tokio

Use the tokio runtime for shared state and concurrency. It's easier to maintain than `std::sync` for most operations.

### Shared State with Tokio

```rust
use tokio::sync::{RwLock, Mutex, mpsc};
use std::sync::Arc;

/// Thread-safe cache using tokio primitives.
#[derive(Clone)]
pub struct Cache<K, V> {
    inner: Arc<RwLock<HashMap<K, V>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        self.inner.read().await.get(key).cloned()
    }

    pub async fn insert(&self, key: K, value: V) {
        self.inner.write().await.insert(key, value);
    }
}
```

### Message Passing with Tokio Channels

```rust
use tokio::sync::mpsc;

pub struct Worker {
    tx: mpsc::Sender<Job>,
}

impl Worker {
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                process_job(job).await;
            }
        });

        Self { tx }
    }

    pub async fn submit(&self, job: Job) -> Result<(), WorkerError> {
        self.tx.send(job).await.map_err(|_| WorkerError::Shutdown)
    }
}
```

### Async Mutex for Complex State

```rust
use tokio::sync::Mutex;

pub struct StateMachine {
    state: Arc<Mutex<State>>,
}

impl StateMachine {
    pub async fn transition(&self, event: Event) -> Result<(), StateError> {
        let mut state = self.state.lock().await;
        *state = state.apply(event)?;
        Ok(())
    }
}
```

---

## 6. Documentation Standards

Keep docs **concise** — meet clippy's minimum requirements without being overly verbose.

```rust
/// Calculates the transaction fee in satoshis.
///
/// # Panics
///
/// Panics on overflow (critical bitcoin path).
pub fn calculate_fee(size: u64, rate: u64) -> u64 { ... }

/// Fetches account balance.
///
/// # Errors
///
/// Returns error if account not found or database unavailable.
pub async fn get_balance(id: AccountId) -> Result<Satoshis, Error> { ... }
```

For unsafe code, always document safety requirements:

```rust
/// # Safety
///
/// Caller must ensure `ptr` is valid and properly aligned.
pub unsafe fn from_raw(ptr: *const u8, len: usize) -> &[u8] { ... }
```

---

## 7. File Size Limit

**Keep files under 1000 lines of code.**

When a file grows large:
1. Extract related types into submodules
2. Split by domain concept, not arbitrary line count
3. Use `mod.rs` to re-export public items

```rust
// src/transaction/mod.rs
mod builder;
mod signing;
mod validation;

pub use builder::TransactionBuilder;
pub use signing::{sign, verify};
pub use validation::validate;
```

---

## 8. API Design Guidelines

### Function Signatures

```rust
// Accept generic inputs, return concrete types
pub fn process(input: impl AsRef<str>) -> ProcessedData { ... }

// Use Option for optional returns
pub fn find(id: &Id) -> Option<&Item> { ... }

// Use Result for fallible operations
pub fn parse(input: &str) -> Result<Config, ParseError> { ... }

// Return iterators for collections
pub fn all_items(&self) -> impl Iterator<Item = &Item> { ... }
```

### Derive Macro Order

```rust
// Consistent ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Point { x: i32, y: i32 }
```

### Builder Pattern

```rust
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
}

impl ConfigBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn host(mut self, h: impl Into<String>) -> Self {
        self.host = Some(h.into());
        self
    }

    pub fn build(self) -> Result<Config, BuildError> {
        Ok(Config {
            host: self.host.ok_or(BuildError::MissingField("host"))?,
            port: self.port.unwrap_or(8080),
        })
    }
}
```

---

## 9. Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_calculation_panics_on_overflow() {
        let result = std::panic::catch_unwind(|| {
            calculate_fee(u64::MAX, u64::MAX, u64::MAX)
        });
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cache_insert_and_get() {
        let cache = Cache::new();
        cache.insert("key", "value").await;
        assert_eq!(cache.get(&"key").await, Some("value"));
    }
}
```

---

## 10. Anti-Patterns to Avoid

| Anti-Pattern | Problem | Solution |
|--------------|---------|----------|
| `.unwrap()` in non-critical path | Unexpected panics | Return `Result` |
| `.unwrap_or_default()` | Silent failures | Explicit error handling |
| `std::sync` for async code | Blocking the runtime | Use `tokio::sync` |
| Files > 1000 LOC | Hard to navigate | Split into modules |
| Stringly-typed APIs | No compile-time safety | Newtypes |
| Missing overflow checks (bitcoin) | Fund loss | `checked_*` + panic |
| Verbose documentation | Noise | Concise, clippy-minimum |
| Deep nesting (>4 levels) | Hard to follow | Early returns, extract fns |

---

## 11. Checklist Before Submitting

- [ ] `cargo fmt` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] `cargo doc --no-deps` builds without warnings
- [ ] All files < 1000 LOC
- [ ] Bitcoin paths panic on overflow/underflow
- [ ] API endpoints return errors (no panics)
- [ ] No `unwrap_or_default` without explicit justification
- [ ] Types leverage generics, traits, newtypes appropriately
- [ ] Concurrency uses tokio primitives

---

## 12. Quick Reference Commands

```bash
# Format
cargo fmt

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Test
cargo test --workspace

# Doc check
cargo doc --no-deps --workspace

# Security audit
cargo audit

# Dependency licenses
cargo deny check
```
