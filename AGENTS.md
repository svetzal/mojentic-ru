# Usage Rules for Mojentic Rust

**IMPORTANT**: Consult these usage rules early and often when working with this Rust project.
Review these guidelines to understand the correct patterns, conventions, and best practices.

## Rust Core Usage Rules

### Error Handling
- Use `Result<T, E>` for operations that can fail
- Use `?` operator for error propagation
- Use `thiserror` for library errors and `anyhow` for application errors
- Prefer custom error types over string errors
- Don't use `unwrap()` or `expect()` in library code - always return `Result`
- In examples/tests, `expect()` is acceptable with descriptive messages

### Async/Await
- Use `async/await` for I/O-bound operations
- Prefer `tokio::spawn` for CPU-bound work in async contexts
- Set timeouts for network operations
- Use `tokio::select!` for concurrent operations with cancellation
- Implement `Send + Sync` bounds for types crossing await points
- Use `Arc` for shared ownership across async tasks

### Common Mistakes to Avoid
- Don't clone unnecessarily - use references and borrowing
- Avoid `.clone()` in hot paths - consider `Rc`, `Arc`, or restructuring
- Don't use `String` when `&str` is sufficient
- Avoid allocations in loops - pre-allocate or use iterators
- Don't use `Box<dyn Trait>` when generic parameters work
- Prefer owned types in public APIs over references (easier to use)
- Don't make everything public - use `pub(crate)` for internal APIs

### Type Design
- Use the type system to prevent invalid states
- Prefer newtype patterns for domain primitives: `struct UserId(String)`
- Use builder pattern for complex construction: `SomeType::builder().field(x).build()`
- Implement `Default` when there's a sensible default
- Use `#[non_exhaustive]` on enums/structs that may grow
- Derive common traits: `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`

### Collections and Iterators
- Use iterators over loops when possible - they're more idiomatic and often faster
- Chain iterator methods: `.filter().map().collect()`
- Use `collect()` with type hints: `let v: Vec<_> = iter.collect()`
- Prefer `into_iter()` over `iter()` when consuming is acceptable
- Use `iter()` for borrowed iteration, `iter_mut()` for mutable iteration

### Testing
- Use `#[cfg(test)]` for test modules
- Use `#[test]` for test functions
- Use `assert_eq!` and `assert_ne!` for equality tests
- Use `assert!` for boolean conditions
- Use `#[should_panic]` for tests expecting panics
- Run specific test: `cargo test test_name`
- Run tests with output: `cargo test -- --nocapture`
- Run doc tests: `cargo test --doc`

### Documentation
- Write doc comments with `///` for public items
- Include examples in doc comments - they're tested by `cargo test`
- Document errors in `# Errors` section
- Document panics in `# Panics` section
- Document safety in `# Safety` section for unsafe code
- Use `#![deny(missing_docs)]` for libraries requiring full documentation

### Cargo and Dependencies
- Keep dependencies minimal - every dependency adds compile time and risk
- Use `cargo update` to update dependencies within semver compatibility
- Use `cargo outdated` to check for major version updates
- Review dependency trees with `cargo tree`
- Use workspace inheritance for shared dependencies in multi-crate projects

## Quality Guidelines

### MANDATORY Pre-Commit Quality Gates

**STOP**: Before considering ANY work complete or committing code, you MUST run ALL quality checks:

```bash
# Complete quality gate check (run this EVERY TIME)
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test && \
cargo deny check
```

**Why `--all-targets` matters**: This flag ensures examples, tests, and benchmarks are checked, not just library code. Examples are executable documentation - if they don't compile, users cannot learn from them.

**If any check fails**:
- STOP immediately
- Fix the root cause (don't suppress warnings)
- Re-run all checks
- Only proceed when all pass

### Code Quality
- Run `cargo fmt` to format code consistently
- Run `cargo clippy --all-targets --all-features -- -D warnings` for strict linting
- Fix all clippy warnings before committing
- Use `#[allow(clippy::lint_name)]` sparingly and document why

### Testing
- Write unit tests for new functions
- Write integration tests for public APIs in `tests/` directory
- Run `cargo test` after writing/updating tests
- Run `cargo test --all-features` to test all feature combinations
- Run `cargo tarpaulin` for code coverage (aim for >80% coverage)
- Coverage reports are in `coverage/` directory

### Performance
- Profile before optimizing: use `cargo flamegraph` or `perf`
- Use `cargo bench` for benchmarking with criterion
- Prefer zero-cost abstractions
- Be aware of allocation costs
- Use `Cow<str>` when you might need to own or borrow

## Security Guidelines

### Dependency Security
- Run `cargo deny check` to check for:
  - Security advisories (vulnerabilities in dependencies)
  - License compliance issues
  - Banned dependencies
  - Multiple versions of the same crate
- Run `cargo audit` for additional security vulnerability scanning
- Check for outdated dependencies: `cargo outdated`
- Review security advisories: https://rustsec.org/
- Keep dependencies up to date, especially security patches
- Use `cargo update` regularly for patch updates

### Secure Coding
- Validate all inputs, especially from external sources
- Use constant-time comparison for secrets: `subtle::ConstantTimeEq`
- Clear sensitive data from memory: use `zeroize` crate
- Avoid `unsafe` code unless absolutely necessary
- If using `unsafe`, document invariants and safety requirements
- Use `#![forbid(unsafe_code)]` if crate should never use unsafe

### Configuration
- Don't commit secrets or API keys
- Use environment variables for sensitive configuration
- Use `.env` files for development (gitignored)
- Validate configuration at startup
- Use strong types for configuration (not stringly-typed)

## Project-Specific Guidelines

### Mojentic Framework
- This is an LLM integration framework for Rust
- Follow async patterns consistently
- Keep gateway implementations separate and feature-gated
- Use `Result<T, MojenticError>` for all public APIs
- Implement `Debug`, `Clone`, `Serialize`, `Deserialize` for message types
- Write examples for new features in `examples/` directory

### Features
- Default feature: `["ollama"]`
- Available features: `ollama`, `openai`, `anthropic`, `full`
- Test all feature combinations in CI
- Keep feature gates minimal and orthogonal

## Release Process

### Versioning
- Follow semantic versioning (semver): MAJOR.MINOR.PATCH
- Update version in `Cargo.toml`
- Update `CHANGELOG.md` with release notes

### Publishing a Release

**The release pipeline is fully automated.** When you create a GitHub release with a `v*` tag, the CI/CD workflow will:
1. Run all quality checks (fmt, clippy, test, security audit)
2. Build and publish the package to crates.io
3. Deploy documentation to GitHub Pages

#### Steps to Release

```bash
# 1. Update version in Cargo.toml (e.g., version = "1.1.0")

# 2. Update CHANGELOG.md
#    - Add new version section with date: ## [1.1.0] - YYYY-MM-DD
#    - Document all changes under appropriate headers (Added, Changed, Fixed, etc.)

# 3. Commit and push
git add Cargo.toml CHANGELOG.md
git commit -m "chore: prepare v1.1.0 release"
git push origin main

# 4. Create GitHub release (this triggers the publish)
gh release create v1.1.0 \
  --title "v1.1.0 - Release Title" \
  --notes "## What's New

- Feature 1
- Feature 2

See [CHANGELOG.md](CHANGELOG.md) for full details."
```

**Important**: The tag MUST start with `v` (e.g., `v1.0.0`) to trigger crates.io publishing.

The pipeline will automatically publish to crates.io using the `CRATES_IO_TOKEN` secret configured in the repository.

### CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/build.yml`) runs:

| Trigger | Quality Checks | Docs Deploy | Crates Publish |
|---------|---------------|-------------|----------------|
| Push to main | ✅ | ❌ | ❌ |
| Pull request | ✅ | ❌ | ❌ |
| Release (`v*` tag) | ✅ | ✅ | ✅ |
| Release (other tag) | ✅ | ✅ | ❌ |

**Required GitHub Secrets:**
- `CRATES_IO_TOKEN` - crates.io API token for publishing (get from https://crates.io/settings/tokens)

### Crates.io Publishing Requirements

1. **CRATES_IO_TOKEN secret**: Must be configured in GitHub repository settings
   - Go to https://crates.io/settings/tokens
   - Create a new token with publish access
   - Add to GitHub: Settings → Secrets and variables → Actions → New repository secret

2. **Package configuration** (`Cargo.toml`) - Already configured:
   - `name`, `version`, `description`, `license` ✅
   - `repository` pointing to GitHub repo ✅
   - `documentation` pointing to docs ✅
   - `keywords` and `categories` ✅

### Pre-Release Checklist

Before creating a release:
- [ ] All tests pass: `cargo test --all-features`
- [ ] Format check passes: `cargo fmt --check`
- [ ] Clippy passes: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Security audit clean: `cargo deny check`
- [ ] Docs build: `cargo doc --no-deps --all-features`
- [ ] Version updated in `Cargo.toml`
- [ ] CHANGELOG.md updated
- [ ] Changes committed and pushed to main

## Useful Commands

### Development
```bash
# Format code
cargo fmt

# Check for errors without building
cargo check

# Build
cargo build

# Build with all features
cargo build --all-features

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test

# Run tests with coverage
cargo tarpaulin

# Run examples
cargo run --example simple_llm

# Check security advisories
cargo deny check
cargo audit

# Check for outdated dependencies
cargo outdated

# Update dependencies (within semver)
cargo update

# View dependency tree
cargo tree
```

### Before Committing
```bash
# Run all quality checks
cargo fmt && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --all-features && \
cargo deny check && \
cargo audit
```

## Resources

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Tokio Documentation](https://tokio.rs/)
- [Serde Documentation](https://serde.rs/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/)

---

*Last Updated: November 2025 • Rust 1.80+ • Tokio 1.x*
