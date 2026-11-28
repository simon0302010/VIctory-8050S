#!/usr/bin/env bash
# rep2.sh <label>: one run, the inmate at world+30 and the diner at world+300, the title's GPU-time lines.
cd /home/simon/Dokumente/Rust/victory/VIctory-Belobug-AMD
R=~/.local/share/victory-sandbox/runtime/logs; d=rep2-$1-$(date +%s); mkdir -p $R/$d
VICTORY_SHADER_KEY=source VICTORY_CAPTURE_WORLD=1 VICTORY_CAPTURE_WORLD_ALSO=300 VICTORY_CAPTURE="Z:\\victory\\logs\\$d" timeout 600 linux/bench-scene.sh $1 14 | head -1
L=$R/$(cat $R/latest.txt); w=$(sed 's/\x1b\[[0-9;]*m//g' $L | grep -m1 "first frame of 500" | grep -o "frame=[0-9]*" | cut -d= -f2)
echo "  world=$w; title GPU time lines:"; sed 's/\x1b\[[0-9;]*m//g' $L | grep "the title's GPU time" | sed -E 's/.*say: //' | tail -8 | sed 's/^/    /'
cd $R/$d && montage frame-$((w+30)).png frame-$((w+300)).png -tile 2x -geometry 480x270+2+2 ${CHECK_OUT:-/tmp}/$d.png && echo "  sheet $d.png"
