#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 3 ]; then
    echo "Usage: $0 <input> <outdir> [--from mm:ss] [--to mm:ss] <profile> [<profile> ...]"
    echo ""
    echo "Render the same audio file with multiple vividspektrum presets."
    echo "Each output PNG uses the profile's [image] settings and the audio duration"
    echo "(--auto-width) so one PNG column equals one live-window column."
    echo ""
    echo "Examples:"
    echo "  $0 song.mp3 showcase default personal bass-heavy"
    echo "  $0 song.mp3 showcase --from 00:30 --to 01:30 default personal"
    echo "  SPEKTRUM_BIN=target/release/audio_to_png $0 song.mp3 showcase default personal"
    exit 1
fi

input="$1"
outdir="$2"
shift 2

from_arg=""
to_arg=""
profiles=()

while [ $# -gt 0 ]; do
    case "$1" in
        --from)
            shift
            from_arg="$1"
            shift
            ;;
        --to)
            shift
            to_arg="$1"
            shift
            ;;
        *)
            profiles+=("$1")
            shift
            ;;
    esac
done

extra_args=()
[ -n "$from_arg" ] && extra_args+=("--from" "$from_arg")
[ -n "$to_arg" ] && extra_args+=("--to" "$to_arg")

mkdir -p "$outdir"

bin="${SPEKTRUM_BIN:-target/release/audio_to_png}"
if ! command -v "$bin" >/dev/null 2>&1 && [ ! -x "$bin" ]; then
    echo "audio_to_png not found at '$bin'. Build it with:"
    echo "  cargo build --release --bin audio_to_png"
    echo "or set SPEKTRUM_BIN to the binary path."
    exit 1
fi

if [ ${#profiles[@]} -eq 0 ]; then
    echo "No profiles given."
    exit 1
fi

for profile in "${profiles[@]}"; do
    profile_file="presets/${profile}.toml"
    if [ ! -f "$profile_file" ]; then
        echo "Skipping unknown profile '$profile' (expected $profile_file)"
        continue
    fi
    out="$outdir/${profile}.png"
    echo "==> Rendering $profile -> $out"
    "$bin" --config "$profile_file" --auto-width "${extra_args[@]}" "$input" "$out"
done

echo "Done. Outputs in $outdir"
