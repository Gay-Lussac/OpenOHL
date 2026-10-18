## What and why

<!-- What does this change and why? Link the issue: Closes #… -->

## Checklist

- [ ] Tests added or updated (`cargo test --workspace`)
- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` pass
- [ ] New source files carry the MPL-2.0 header

For engineering calculations:

- [ ] Reference cited (standard clause, paper, textbook page)
- [ ] Validation case(s) added in `benchmarks/` with tolerance and source
- [ ] Theory note added or updated in `docs/theory/`
- [ ] Iterative or long-running code has exit clauses (`ohl_core::budget`): iteration limit, time limit, cancellation, and an error that explains why it stopped
- [ ] Expected duration documented; benchmark added if performance-sensitive
