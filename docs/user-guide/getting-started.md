# Getting started

OLS CAD is not usable for line design yet. These tools work today.

## Command-line tool

```bash
cargo run -p ols-cli -- info               # version information
cargo run -p ols-cli -- validate           # run all validation cases in benchmarks/
cargo run -p ols-cli -- validate --list    # calculations available to cases
cargo run -p ols-cli -- plugin info plugins/examples/hello-wasm
```

`ols validate` exits with a non-zero code if any case fails or exceeds its time limit, and `--report report.md` writes a Markdown report.

## Python

```bash
pip install ./crates/ols-py
python -c "import ols; print(ols.convert('length', '1000 ft', 'm'))"   # 304.8
```

Quantities given as text must always carry a unit (`"1000 ft"`, `"16 N/m"`, `"-20 °C"`); a bare number is rejected rather than guessed.
