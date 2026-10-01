# Performance and time budgets

## Benchmarks

Performance-sensitive code has [criterion](https://github.com/bheisler/criterion.rs) benchmarks in `crates/<crate>/benches/`:

```bash
cargo bench --workspace --bench '*'
```

CI runs them on every change and fails if a benchmark becomes more than 1.5× slower than the last result on `main`. Shared CI machines are noisy, so the threshold is deliberately loose; local runs give precise numbers.

Example: `units/total_load_typed_10k` vs `units/total_load_raw_10k` check that typed quantities cost nothing compared with plain `f64`.

## Time budgets and exit clauses

Nothing may run forever (see the design goals). In code:

```rust
use std::time::Duration;
use ols_core::budget::{Budget, CancelToken};

let budget = Budget::new()
    .max_iterations(100)                     // exit clause 1: iterations
    .time_limit(Duration::from_millis(50))   // exit clause 2: wall-clock time
    .cancel_token(CancelToken::new());       // exit clause 3: user cancel
let mut run = budget.start();
loop {
    run.step()?;           // returns Err(Stop) explaining why it stopped
    // … one iteration …
    # break;
}
# Ok::<(), ols_core::budget::Stop>(())
```

Guidelines:

1. **Estimate** the expected duration of every potentially slow function and write it in its documentation.
2. Set the limit with a safety margin (≈10× the estimate) so that hitting it means a real problem.
3. When a limit is hit, **report why**: which limit, iterations done, elapsed time, and the solver's last state (e.g. residual). This is what lets us understand and fix the cause.
4. Never return an unconverged result as if it were valid.
