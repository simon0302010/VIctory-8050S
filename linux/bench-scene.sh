#!/usr/bin/env bash
# Runs the title in victory-sandbox up to the intro's 3D scene (Enter pressed
# every 5 s through VICTORY_KEYS, which needs a `diagnostics` build), lets it
# play until LINES frame-rate lines with 100+ draws a frame were logged, ends
# it and prints a summary of those lines. Extra VICTORY_* switches come from
# the environment, e.g.:
#
#   VICTORY_MAP_THREADS=1 linux/bench-scene.sh map1
#   BENCH_RUN_ARGS=--rt linux/bench-scene.sh rt   (more arguments for `victory-sandbox run`)
#
# Usage: bench-scene.sh <label> [lines]   (lines: 10 unless given)
set -u
label=${1:-run}
lines=${2:-10}
logs=~/.local/share/victory-sandbox/runtime/logs
old=$(cat "$logs/latest.txt" 2>/dev/null)
keys=$(seq 10 5 200 | sed 's/$/:Enter/' | paste -sd,)

# (The renderer patches come from victory's Linux defaults, apps/victory/src/main.rs.)

VICTORY_KEYS=$keys timeout 600 victory-sandbox run --no-audio ${BENCH_RUN_ARGS:-} >/dev/null 2>&1 &
runner=$!

in_game() { grep -cE 'INFO.*victory_gpu::host.* fps, .* [0-9]{3,} draws/frame' "$1" 2>/dev/null; true; }
log=""
for _ in $(seq 1 200); do
    now=$(cat "$logs/latest.txt" 2>/dev/null)
    if [ "$now" != "$old" ]; then
        log=$logs/$now
        [ "$(in_game "$log")" -ge "$lines" ] 2>/dev/null && break
    fi
    sleep 3
done
pkill -f '[v]ictory.exe run'
wait "$runner" 2>/dev/null

[ -n "$log" ] || { echo "$label: no log"; exit 1; }
sed 's/\x1b\[[0-9;]*m//g' "$log" \
  | grep -E 'INFO victory_gpu::host: [0-9.]+ fps, .* [0-9]{3,} draws/frame' \
  | sed -E 's/.* ([0-9.]+) ms\/frame \(([0-9.]+) preparing: copies ([0-9.]+) ms [0-9.]+ MB, textures ([0-9.]+) ms .*; ([0-9.]+) on the GPU.*/\1 \2 \3 \4 \5/' \
  | awk -v label="$label" -v file="$log" '
      { n++; ms[n]=$1; p+=$2; c+=$3; t+=$4; g+=$5; s+=$1 }
      END {
        if (!n) { print label ": no in-game lines in " file; exit }
        asort(ms); med = ms[int((n+1)/2)]
        printf "%s: %d lines, ms/frame mean %.0f median %.0f; per frame preparing %.0f (copies %.0f, textures %.0f), GPU %.0f\n  %s\n",
               label, n, s/n, med, p/n, c/n, t/n, g/n, file
      }'
