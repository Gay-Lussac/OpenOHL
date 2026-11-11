#!/bin/bash
# Builds the macOS disk image of OpenOHL on this Mac:
#
#   tools/package-macos.sh              build, then package
#   tools/package-macos.sh --no-build   package what the last run built
#   tools/package-macos.sh --python     with the Python console (the user
#                                       then needs the same Python installed)
#   tools/package-macos.sh --keep       keep the build folder afterwards
#   tools/package-macos.sh --universal  for Apple Silicon and Intel Macs in
#                                       one image (builds twice; needs the
#                                       Intel target once:
#                                       rustup target add x86_64-apple-darwin)
#
# Without --universal the image is for this Mac's processor only: it runs
# on every Mac with the same kind of processor (macOS 11 or later), not on
# the other kind.
#
# Writes into dist/ (git-ignored):
#   OpenOHL-<version>-macos-<arch>.dmg       OHL-Design.app, OHL-Structure.app
#                                            and a link to Applications
#   OpenOHL-<version>-macos-<arch>-cli.tar.gz  the ohl command line, examples
#
# Each application holds its programs (OHL-Design also OHL-Structure and
# ohl, so it finds them beside itself), the shipped libraries, the
# examples, the manual and the licence. Signed ad hoc only (no Apple
# developer certificate): on another Mac the first start needs
# right-click > Open, or System Settings > Privacy & Security > Open
# anyway. Say so in the release notes.
#
# It builds in target/package, apart from target/release, so a build
# started with `cargo run --release` is neither used nor replaced. A cold
# build takes about 10 to 15 minutes; the folder (several GB) is removed
# afterwards unless --keep.
#
# Needs only what macOS and the Rust toolchain bring: cargo, sips,
# iconutil, codesign, hdiutil.
set -euo pipefail

BUILD=1
PYTHON=0
KEEP=0
UNIVERSAL=0
for a in "$@"; do
    case "$a" in
        --no-build) BUILD=0 ;;
        --python) PYTHON=1 ;;
        --keep) KEEP=1 ;;
        --universal) UNIVERSAL=1 ;;
        -h|--help) sed -n '2,35p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $a (see --help)" >&2; exit 2 ;;
    esac
done

cd "$(dirname "$0")/.."
ROOT=$(pwd)
[ "$(uname -s)" = "Darwin" ] || { echo "this script runs on macOS" >&2; exit 2; }
if [ "$UNIVERSAL" = 1 ] && [ "$PYTHON" = 1 ]; then
    echo "--universal with --python: not offered (it would need a Python built for both processors)" >&2
    exit 2
fi
for t in cargo sips iconutil codesign hdiutil; do
    command -v "$t" > /dev/null || { echo "missing tool: $t" >&2; exit 2; }
done

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$VERSION" ] || { echo "no version in Cargo.toml" >&2; exit 2; }
ARCH=$(uname -m)
[ "$UNIVERSAL" = 1 ] && ARCH=universal
BUILD_ID=$(git rev-parse --short=12 HEAD 2> /dev/null || echo unknown)
git diff --quiet HEAD 2> /dev/null || BUILD_ID="$BUILD_ID+changes"
TARGET="$ROOT/target/package"
BIN="$TARGET/release"
[ "$UNIVERSAL" = 1 ] && BIN="$TARGET/universal"
TRIPLES="aarch64-apple-darwin x86_64-apple-darwin"
DIST="$ROOT/dist"
NAME="OpenOHL-$VERSION-macos-$ARCH"
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/openohl-package.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT

echo "OpenOHL $VERSION ($BUILD_ID), $ARCH"

# Builds the three programs, for one processor (`--target …`) or this
# Mac's (no argument).
build() {
    if [ "$PYTHON" = 1 ]; then
        cargo build --release --locked --target-dir "$TARGET" "$@" -p ohl-app
    else
        cargo build --release --locked --target-dir "$TARGET" "$@" -p ohl-app --no-default-features
    fi
    cargo build --release --locked --target-dir "$TARGET" "$@" -p ohl-structure-app -p ohl-cli
}

if [ "$BUILD" = 1 ] && [ "$UNIVERSAL" = 1 ]; then
    for t in $TRIPLES; do
        rustup target list --installed 2> /dev/null | grep -qx "$t" || {
            echo "the build target $t is not installed: rustup target add $t" >&2
            exit 2
        }
    done
    for t in $TRIPLES; do
        echo "building for $t (a cold build takes 10 to 15 minutes)…"
        build --target "$t"
    done
    # One program for both processors from the two builds.
    mkdir -p "$BIN"
    for b in ohl-design ohl-structure ohl; do
        /usr/bin/lipo -create -output "$BIN/$b" \
            "$TARGET/aarch64-apple-darwin/release/$b" "$TARGET/x86_64-apple-darwin/release/$b"
    done
elif [ "$BUILD" = 1 ]; then
    echo "building (a cold build takes 10 to 15 minutes)…"
    build
fi
for b in ohl-design ohl-structure ohl; do
    [ -x "$BIN/$b" ] || { echo "not built: $BIN/$b (run without --no-build)" >&2; exit 1; }
done

# Signs ad hoc, without the note that the linker's own signature is
# replaced.
sign() {
    codesign --force --sign - "$1" 2> "$STAGE/codesign.log" || {
        cat "$STAGE/codesign.log" >&2
        exit 1
    }
}

# An .icns from an application icon (SVG): every size macOS asks for.
icns() { # svg out.icns
    local set="$STAGE/$(basename "$2" .icns).iconset"
    mkdir -p "$set"
    for s in 16 32 128 256 512; do
        sips -s format png -z "$s" "$s" "$1" --out "$set/icon_${s}x${s}.png" > /dev/null
        local d=$((s * 2))
        sips -s format png -z "$d" "$d" "$1" --out "$set/icon_${s}x${s}@2x.png" > /dev/null
    done
    iconutil -c icns "$set" -o "$2"
}

# An application bundle: its name, its identifier, its main program, its
# icon, then the other programs it holds.
bundle() { # "OHL-Design" org.openohl.design ohl-design icon.svg [more programs…]
    local name="$1" app="$STAGE/dmg/$1.app" id="$2" main="$3" icon="$4"
    shift 4
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    for b in "$main" "$@"; do
        cp "$BIN/$b" "$app/Contents/MacOS/"
    done
    icns "$icon" "$app/Contents/Resources/icon.icns"
    # What the programs look for beside themselves (libraries, examples,
    # the manual): kept in Resources, linked from MacOS where they look.
    cp -R libraries examples "$app/Contents/Resources/"
    mkdir -p "$app/Contents/Resources/docs"
    cp -R docs/user-guide docs/tutorial docs/theory docs/images "$app/Contents/Resources/docs/" 2> /dev/null || true
    cp LICENSE "$app/Contents/Resources/"
    for d in libraries examples docs; do
        ln -s "../Resources/$d" "$app/Contents/MacOS/$d"
    done
    cat > "$app/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$id</string>
    <key>CFBundleExecutable</key><string>$main</string>
    <key>CFBundleIconFile</key><string>icon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$BUILD_ID</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
</dict>
</plist>
EOF
    # Ad hoc: no certificate; the other programs first, then the bundle
    # (which signs its main program).
    for b in "$@"; do
        sign "$app/Contents/MacOS/$b"
    done
    sign "$app"
    codesign --verify --strict "$app"
}

mkdir -p "$STAGE/dmg" "$DIST"
bundle "OHL-Design" org.openohl.design ohl-design assets/brand/ohl-design-icon.svg ohl-structure ohl
bundle "OHL-Structure" org.openohl.structure ohl-structure assets/brand/ohl-structure-icon.svg
ln -s /Applications "$STAGE/dmg/Applications"

rm -f "$DIST/$NAME.dmg"
hdiutil create -quiet -volname "OpenOHL $VERSION" -srcfolder "$STAGE/dmg" -format UDZO "$DIST/$NAME.dmg"

# The command line on its own, with the examples.
CLI="$STAGE/$NAME-cli"
mkdir -p "$CLI"
cp "$BIN/ohl" LICENSE README.md "$CLI/"
cp -R examples libraries "$CLI/"
sign "$CLI/ohl"
tar -czf "$DIST/$NAME-cli.tar.gz" -C "$STAGE" "$NAME-cli"

if [ "$BUILD" = 1 ] && [ "$KEEP" = 0 ]; then
    rm -rf "$TARGET"
fi

echo
echo "written:"
ls -lh "$DIST/$NAME.dmg" "$DIST/$NAME-cli.tar.gz" | awk '{print "  " $5 "  " $9}'
echo "unsigned (ad hoc): on another Mac the first start needs right-click > Open."
