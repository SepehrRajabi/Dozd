#!/usr/bin/env bash
# Sweeps `dozd headless --class` across all pirate classes and writes one CSV
# row per run to stdout (or -o FILE), via the binary's own `--csv` output.
# Build the release binary first:
#   cargo build --release
#
# Usage: tools/sweep.sh [-o out.csv] [-- EXTRA HEADLESS ARGS]
# Example: tools/sweep.sh -o sweep.csv -- --runs 200 --crew 1 --bot random --time 300 --hz 60

set -euo pipefail

cd "$(dirname "$0")/.."
BIN="target/release/dozd"
CLASSES=(gunner engineer bulwark hacker)
OUT=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -o) OUT="$2"; shift 2 ;;
        --) shift; break ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done
EXTRA_ARGS=("$@")

if [[ ! -x "$BIN" ]]; then
    echo "missing $BIN — run: cargo build --release" >&2
    exit 1
fi

{
    header_written=0
    for class in "${CLASSES[@]}"; do
        rows=$("$BIN" headless --class "$class" --csv "${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}")
        if [[ $header_written -eq 0 ]]; then
            echo "$rows"
            header_written=1
        else
            tail -n +2 <<<"$rows"
        fi
    done
} | if [[ -n "$OUT" ]]; then cat > "$OUT"; else cat; fi
