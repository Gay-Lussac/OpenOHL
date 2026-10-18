#!/bin/zsh
# Double-click (macOS) to build and start OLS Structure, the structure
# application, on its start page: Open a structure…, the recent files, or
# New structure…. The first start after a code change compiles for about a
# minute; later starts take a few seconds.

cd "${0:A:h}" || exit 1
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

echo "Building and starting OLS Structure…"
cargo build --release -p ols-structure-app && cargo run --release -p ols-structure-app --bin ols-structure
status=$?

if [ $status -ne 0 ]; then
    echo
    echo "OLS Structure stopped with an error (exit code $status) — see the messages above."
    read -k 1 "?Press any key to close this window."
fi
