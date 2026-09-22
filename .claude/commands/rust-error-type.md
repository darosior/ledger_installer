---
description: Generate production-ready error types
allowed-tools: Read, Write
---

# Generate Error Type

Create error types for: $ARGUMENTS

Follow these rules from our standards:
1. Use `#[derive(Debug, Error)]` with thiserror
2. Add `#[non_exhaustive]` for public enums
3. Include `#[error("...")]` with descriptive messages
4. Implement `From` for underlying errors
5. Add doc comments explaining each variant
6. Include `# Errors` section in module docs
