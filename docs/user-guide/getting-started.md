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

## Sag-tension of a tension section

```bash
cargo run -p ols-cli -- sagtension examples/sagtension/drake-section.toml
cargo run -p ols-cli -- sagtension examples/sagtension/drake-section.toml --units us --csv chart.csv
```

The job file names the cable (from `libraries/cables/`), the spans (horizontal length and rise), the weather cases (temperature, radial ice, wind pressure), and either a fixed stringing tension or tension criteria. The report gives the ruling span, the governing stringing tension with the utilisation of every criterion, the tension and sag in each weather case, and a stringing chart; `--csv` exports the chart in SI units.

## Python

```bash
pip install ./crates/ols-py
python -c "import ols; print(ols.convert('length', '1000 ft', 'm'))"   # 304.8
```

Quantities given as text must always carry a unit (`"1000 ft"`, `"16 N/m"`, `"-20 °C"`); a bare number is rejected rather than guessed.
