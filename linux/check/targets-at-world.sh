#!/usr/bin/env bash
# tg.sh <label>: dump targets at world+30; sheet of final + albedo(06-ish 1280x720 f27) + the 1280x720 f28 targets.
cd /home/simon/Dokumente/Rust/victory/VIctory-Belobug-AMD
R=~/.local/share/victory-sandbox/runtime/logs; d=tg-$1-$(date +%s); mkdir -p $R/$d
VICTORY_SHADER_KEY=source VICTORY_DUMP_TARGETS=1 VICTORY_CAPTURE_WORLD=1 VICTORY_CAPTURE="Z:\\victory\\logs\\$d" timeout 600 linux/bench-scene.sh $1 8 | head -1
L=$R/$(cat $R/latest.txt); w=$(sed 's/\x1b\[[0-9;]*m//g' $L | grep -m1 "first frame of 500" | grep -o "frame=[0-9]*" | cut -d= -f2); f=$((w+30))
cd $R/$d && montage -label '%t' frame-$f.png $(ls | grep -E "^frame-$f-[0-9]+-.*-1280x720-f(27|28|10)\.png" | sort -t- -k3 -n | head -9) -tile 5x -geometry 384x216+2+12 -pointsize 9 ${CHECK_OUT:-/tmp}/$d.png && echo "  sheet $d.png (frame $f)"
# keep only that frame's dumps
find $R/$d -name "frame-*-*-0x*.png" ! -name "frame-$f-*" -delete
