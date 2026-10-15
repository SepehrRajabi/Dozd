#!/usr/bin/env bash
# Builds a release of DOZD for one platform and packs it into `dist/`, ready to
# hand out: a zip with `DOZD.exe` on Windows, a zip with `DOZD.app` on macOS
# (one app for Apple Silicon and Intel), and a tarball with `dozd` on Linux.
#
# usage: tools/package.sh windows|macos|linux
#
# Builds with `cargo build` by default; set CARGO_BUILD to change that, e.g.
# CARGO_BUILD="cargo xwin build" to cross-build for Windows from a Mac.
set -euo pipefail
cd "$(dirname "$0")/.."

platform=${1:?usage: tools/package.sh windows|macos|linux}
build=${CARGO_BUILD:-cargo build}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
stage=target/package/$platform

rm -rf "$stage" && mkdir -p "$stage" dist

case $platform in
windows)
    target=x86_64-pc-windows-msvc
    $build --release --target $target
    cp target/$target/release/dozd.exe "$stage/DOZD.exe"
    out=dist/dozd-$version-windows-x86_64.zip
    rm -f "$out"
    # Macs and Linux have zip; Windows has PowerShell.
    if command -v zip > /dev/null; then
        (cd "$stage" && zip -q "$OLDPWD/$out" DOZD.exe)
    else
        powershell -NoProfile -Command "Compress-Archive -Path '$stage/DOZD.exe' -DestinationPath '$out'"
    fi
    ;;
macos)
    # Older Macs can run it too, not just the one that built it.
    export MACOSX_DEPLOYMENT_TARGET=11.0
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        $build --release --target $target
    done
    app=$stage/DOZD.app
    mkdir -p "$app/Contents/MacOS"
    lipo -create -output "$app/Contents/MacOS/dozd" \
        target/aarch64-apple-darwin/release/dozd target/x86_64-apple-darwin/release/dozd
    sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
    # Ad-hoc signed, so Apple Silicon runs it; it isn't notarized, so the
    # first launch still needs allowing in System Settings.
    codesign --force --deep --sign - "$app"
    out=dist/dozd-$version-macos.zip
    rm -f "$out"
    ditto -c -k --keepParent "$app" "$out"
    ;;
linux)
    target=x86_64-unknown-linux-gnu
    $build --release --target $target
    cp target/$target/release/dozd "$stage/dozd"
    out=dist/dozd-$version-linux-x86_64.tar.gz
    tar -czf "$out" -C "$stage" dozd
    ;;
*)
    echo "unknown platform \`$platform\`: pick windows, macos or linux" >&2
    exit 2
    ;;
esac

echo "packed $out"
