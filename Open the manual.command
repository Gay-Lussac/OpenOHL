#!/bin/zsh
# Double-click (macOS) to open the OpenOHL manual (the mdBook in docs/) in
# the browser. It follows the .md files as they change; close this window
# (or press Ctrl+C) to stop it. The first time, mdBook is installed (about
# a minute).

cd "${0:A:h}" || exit 1
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

if ! command -v mdbook > /dev/null; then
    echo "Installing mdBook (once)…"
    if ! cargo install mdbook; then
        echo
        echo "mdBook could not be installed — see the messages above."
        read -k 1 "?Press any key to close this window."
        exit 1
    fi
fi

echo "Opening the manual…"
mdbook serve --open
rc=$?

if [ $rc -ne 0 ]; then
    echo
    echo "The manual stopped with an error (exit code $rc) — see the messages above."
    read -k 1 "?Press any key to close this window."
fi
