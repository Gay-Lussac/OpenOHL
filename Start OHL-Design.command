#!/bin/zsh
# Double-click (macOS) to build and start OHL-Design, the line application of
# OpenOHL, on its start page: then File > New project or Open. The first
# start after a code change compiles for about a minute; later starts take a
# few seconds. (Formerly "Start OLS CAD.command".)

cd "${0:A:h}" || exit 1
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

echo "Building and starting OHL-Design…"
# Both applications from the same code: Edit… on a structure starts the
# structure application (ohl-structure), which must be next to ohl-design.
cargo build --release -p ols-app -p ols-structure-app && cargo run --release -p ols-app --bin ohl-design
status=$?

if [ $status -ne 0 ]; then
    echo
    echo "OHL-Design stopped with an error (exit code $status) — see the messages above."
    read -k 1 "?Press any key to close this window."
fi
