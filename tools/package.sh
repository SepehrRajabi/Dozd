#!/usr/bin/env bash
# Builds a release of DOZD for one platform and packs it into `dist/`, ready to
# hand out: a zip with `DOZD.exe` on Windows, a zip with `DOZD.app` on macOS
# (one app for Apple Silicon and Intel), and a tarball with `dozd` on Linux:
# x86-64 or RISC-V, for glibc distros or musl ones like Alpine and Void.
# It's the `dist` profile (see Cargo.toml): fully link-time optimised, so it
# takes a while.
#
# usage: tools/package.sh windows|macos|linux|linux-musl|linux-riscv64|linux-riscv64-musl
#
# Builds with `cargo build` by default; set CARGO_BUILD to change that, e.g.
# CARGO_BUILD="cargo xwin build" to cross-build for Windows from a Mac, or
# CARGO_BUILD="cargo zigbuild" to cross-build any of the Linux ones.
set -euo pipefail
cd "$(dirname "$0")/.."

platform=${1:?usage: tools/package.sh windows|macos|linux|linux-musl|linux-riscv64|linux-riscv64-musl}
build=${CARGO_BUILD:-cargo build}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
stage=target/package/$platform

rm -rf "$stage" && mkdir -p "$stage" dist

# A Linux tarball holding `dozd` built for `target`, named after `name`.
linux_tarball() {
    local target=$1 name=$2
    local extra=()
    if [[ $build == *zigbuild* ]]; then
        # musl is linked dynamically (see .cargo/config.toml), and Zig needs telling.
        if [[ $target == *-musl ]]; then
            export "CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_RUSTFLAGS=-C target-feature=-crt-static -C link-arg=-dynamic"
        fi
        # Gamepads link libudev, which a cross-build has none of: link against
        # a stand-in (packaging/linux/udev-stub.c) instead of asking pkg-config.
        local arch=${target%%-*} stub=$PWD/target/udev-stub/$target
        mkdir -p "$stub"
        cargo-zigbuild zig cc -- -target "${arch/riscv64gc/riscv64}-linux-${target##*-}" -shared \
            -Wl,-soname,libudev.so.1 -o "$stub/libudev.so" packaging/linux/udev-stub.c
        extra=(--config "target.$target.libudev.rustc-link-lib=[\"udev\"]"
            --config "target.$target.libudev.rustc-link-search=[\"$stub\"]")
    fi
    $build --profile dist --target "$target" ${extra[@]+"${extra[@]}"}
    cp "target/$target/dist/dozd" "$stage/dozd"
    out=dist/dozd-$version-$name.tar.gz
    tar -czf "$out" -C "$stage" dozd
}

case $platform in
windows)
    target=x86_64-pc-windows-msvc
    $build --profile dist --target $target
    cp target/$target/dist/dozd.exe "$stage/DOZD.exe"
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
        $build --profile dist --target $target
    done
    app=$stage/DOZD.app
    mkdir -p "$app/Contents/MacOS"
    lipo -create -output "$app/Contents/MacOS/dozd" \
        target/aarch64-apple-darwin/dist/dozd target/x86_64-apple-darwin/dist/dozd
    sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
    # Ad-hoc signed, so Apple Silicon runs it; it isn't notarized, so the
    # first launch still needs allowing in System Settings.
    codesign --force --deep --sign - "$app"
    out=dist/dozd-$version-macos.zip
    rm -f "$out"
    ditto -c -k --keepParent "$app" "$out"
    ;;
linux)
    linux_tarball x86_64-unknown-linux-gnu linux-x86_64
    ;;
linux-musl)
    linux_tarball x86_64-unknown-linux-musl linux-musl-x86_64
    ;;
linux-riscv64)
    linux_tarball riscv64gc-unknown-linux-gnu linux-riscv64
    ;;
linux-riscv64-musl)
    linux_tarball riscv64gc-unknown-linux-musl linux-musl-riscv64
    ;;
*)
    echo "unknown platform \`$platform\`: pick windows, macos, linux, linux-musl, linux-riscv64 or linux-riscv64-musl" >&2
    exit 2
    ;;
esac

echo "packed $out"
