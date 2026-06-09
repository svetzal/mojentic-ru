`Cargo.lock` is intentionally gitignored for this library crate (standard practice — consumers pin their own lockfiles). The updates took effect in the working tree and all gates passed.

---

**Summary:**

All 8 updated packages were transitive patch-level bumps — nothing in `Cargo.toml` needed changing:

| Package | Old | New |
|---|---|---|
| `http` | 1.4.1 | 1.4.2 |
| `js-sys` | 0.3.99 | 0.3.100 |
| `wasm-bindgen` (+ macro, shared) | 0.2.122 | 0.2.123 |
| `wasm-bindgen-futures` | 0.4.72 | 0.4.73 |
| `web-sys` | 0.3.99 | 0.3.100 |

**Quality gates:** ✅ format · ✅ clippy · ✅ tests · ✅ build · ✅ coverage (57.26% ≥ 56%) · ✅ `cargo deny` · ✅ `cargo audit`

`Cargo.lock` is gitignored per library-crate convention, so no commit is needed.