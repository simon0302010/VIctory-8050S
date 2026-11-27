#!/usr/bin/env bash
# Runs the bench scene (linux/bench-scene.sh), records 20 s system-wide with perf once in game.
# Usage: linux/perf/record-scene.sh <out dir>  (needs kernel.perf_event_paranoid=-1; copy victory.exe into <out dir> first so symbols match)
set -u
out=$1
cd /home/simon/Dokumente/Rust/victory/VIctory-Belobug-AMD
# (build done by caller)
logs=~/.local/share/victory-sandbox/runtime/logs
old=$(cat $logs/latest.txt)
linux/bench-scene.sh perf 30 > $out/bench.txt 2>&1 &
for _ in $(seq 1 200); do
  now=$(cat $logs/latest.txt)
  if [ "$now" != "$old" ]; then
    n=$(sed 's/\x1b\[[0-9;]*m//g' $logs/$now | grep -cE 'victory_gpu::host: [0-9.]+ fps, .* [0-9]{3,} draws')
    [ "$n" -ge 6 ] && break
  fi
  sleep 3
done
echo "log $now"
pid=$(pgrep -f '[v]ictory.exe run' | head -1)
echo "pid $pid"
: > $out/maps.txt; for p in $(pgrep -f "victory.exe"); do grep -i "victory.exe" /proc/$p/maps 2>/dev/null | head -1 | sed "s/^/$p /" >> $out/maps.txt; done
perf record -a -F 1999 -o $out/perf.data -- sleep 20 2>&1 | tail -2
pkill -f "[v]ictory.exe run"; pkill -f "[b]ench-scene.sh perf"
wait
cat $out/bench.txt
