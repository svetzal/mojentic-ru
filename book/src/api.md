# API Documentation

The full Rust API reference is generated with `cargo doc` and published under `/api/` of the documentation site.

- Visit: /api/
- Crate landing page: /api/mojentic/

Note: During local development, run:

```sh
# Build API docs
cargo doc --no-deps --all-features
# Build the mdBook
mdbook build book
# Preview by serving book/book/ and opening ./api/ in a second tab from target/doc/
```
