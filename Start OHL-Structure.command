#!/bin/zsh
# Double-click (macOS) to build and start OHL-Structure, the structure
# application, on its start page: Open a structure…, the recent files, or
# New structure…. The first start after a code change compiles for about a
# minute; later starts take a few seconds. (Formerly "Start OLS
# Structure.command".)

cd "${0:A:h}" || exit 1
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

echo "Building and starting OHL-Structure…"
cargo build --release -p ohl-structure-app && { sh tools/prune-target.sh target >/dev/null; cargo run --release -p ohl-structure-app --bin ohl-structure; }
rc=$?

if [ $rc -ne 0 ]; then
    echo
    echo "OHL-Structure stopped with an error (exit code $rc) — see the messages above."
    read -k 1 "?Press any key to close this window."
fi
