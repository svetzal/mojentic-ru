# Copilot Development Instructions for mojentic-ru

## Required Quality Tools

This project uses the following tools to maintain code quality, security, and consistency. **These tools MUST be run with every code change session, before committing.**

### Tools Installed

1. **cargo-tarpaulin** - Code coverage analysis
2. **clippy** - Rust linter
3. **rustfmt** - Code formatter
4. **cargo-audit** - Security vulnerability scanner
5. **cargo-deny** - Dependency/license/advisory checker

## Pre-Commit Workflow

Before committing any code changes, you **MUST** run the following checks in order:

### 1. Format Check
```bash
cargo fmt --check
```
If this fails, auto-format the code:
```bash
cargo fmt
```

### 2. Linting
```bash
cargo clippy --all-targets --all-features -- -D warnings
```
All clippy warnings must be resolved before committing.

### 3. Tests
```bash
cargo test
```
All tests must pass.

### 4. Code Coverage
```bash
cargo tarpaulin --out Html --output-dir coverage
```
Review coverage report in `coverage/tarpaulin-report.html`. Aim for high coverage on new code.

**Coverage Focus Areas:**
- Module re-export files (`mod.rs`, `lib.rs` with only `pub mod/pub use`) automatically excluded
- Priority files needing tests:
  - `src/llm/broker.rs` (49 lines, 0% covered)
  - `src/llm/gateways/ollama.rs` (81 lines, 0% covered)
- Well-tested baseline modules:
  - `src/llm/models.rs` (100% covered)
  - `src/llm/tools/tool.rs` (100% covered)
  - `src/llm/gateway.rs` (100% covered)

**Current baseline: 9.72% (14/144 lines)** - Focus on broker and gateway implementations

### 5. Security Audit
```bash
cargo audit
```
No critical vulnerabilities should be present.

### 6. Dependency Check
```bash
cargo deny check
```
Ensure no license violations or dependency issues.

## Development Session Workflow

### Starting a Session
1. Pull latest changes
2. Run quick sanity check: `cargo test`
3. Run `cargo clippy` to identify any existing issues

### During Development
1. Write tests alongside new functionality
2. Run `cargo test` frequently
3. Run `cargo clippy` to catch issues early
4. Format code with `cargo fmt` as you go

### Before Committing
1. Run the complete **Pre-Commit Workflow** (see above)
2. Ensure all checks pass
3. Review changes carefully
4. Write meaningful commit messages

## Continuous Integration Expectations

When CI is configured, it will run:
- `cargo fmt --check` - Formatting verification
- `cargo clippy -- -D warnings` - Linting with no warnings allowed
- `cargo test` - All tests must pass
- `cargo tarpaulin --out Xml` - Coverage reporting
- `cargo audit` - Security scanning
- `cargo deny check` - Dependency verification

## Tool Configuration

### Clippy Configuration
Add to `Cargo.toml` if needed:
```toml
[lints.clippy]
pedantic = "warn"
```

### Deny Configuration
Create `deny.toml` for cargo-deny configuration if needed.

### Tarpaulin Configuration
Create `tarpaulin.toml` if you need custom coverage settings:
```toml
[report]
out-type = ["Html", "Lcov"]

[run]
timeout = "300s"
```

## Standards

- **Test Coverage**: Aim for at least 80% coverage on new code
- **Documentation**: All public APIs must have doc comments
- **Error Handling**: Use proper `Result` types, avoid panics in library code
- **Dependencies**: Keep dependencies minimal and up-to-date
- **Security**: Never commit code with known vulnerabilities

## Quick Reference Commands

```bash
# Full quality check (run before every commit)
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test && \
cargo tarpaulin --out Html --output-dir coverage && \
cargo audit && \
cargo deny check

# Auto-fix formatting and run tests
cargo fmt && cargo test

# View coverage report
open coverage/index.html  # macOS
xdg-open coverage/index.html  # Linux

# Update dependencies
cargo update

# Check for outdated dependencies
cargo outdated  # Install with: cargo install cargo-outdated
```

## Notes for GitHub Copilot

When making code changes:
1. **Always** generate unit tests for new functions/methods
2. **Always** add doc comments to public APIs
3. **Always** run the pre-commit workflow before finalizing changes
4. **Never** commit code that doesn't pass all quality checks
5. **Always** maintain or improve test coverage
6. **Always** handle errors properly with `Result` types
7. **Always** follow Rust idioms and best practices

## Getting Help

- Clippy errors: `rustc --explain <ERROR_CODE>`
- Rust book: https://doc.rust-lang.org/book/
- Rust API guidelines: https://rust-lang.github.io/api-guidelines/
