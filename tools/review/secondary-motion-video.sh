#!/usr/bin/env bash
# Side-by-side videos for the owner of roadmap 119800: a hand-keyed walk, physics off, without and
# with `layer.secondary`. Run from the built checkout:
#   tools/review/secondary-motion-video.sh <folder under the main checkout's target/shots>
set -euo pipefail
out="${1:?give the folder for the videos}"
ac="${AC:-target/release/ac}"
ffmpeg="${AC_FFMPEG:-C:/tools/ffmpeg/bin/ffmpeg.exe}"
mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

walk='{"command":"physics.set","params":{"on":false}},
{"command":"pose.set_root","params":{"root":[0,0,0.945]}},
{"command":"key.set","params":{"interp":"smooth"}},
{"command":"frame.set","params":{"frame":80}},
{"command":"pose.set_root","params":{"root":[0,-3.0,0.945]}},
{"command":"key.set","params":{"interp":"smooth"}},
{"command":"anim.steps","params":{"from":0,"to":80,"steps":6,"fill":"none"}},
{"command":"frame.set","params":{"frame":110}},
{"command":"key.set","params":{"interp":"smooth"}}'
echo "[$walk]" > "$work/before.json"
echo "[$walk,{\"command\":\"layer.secondary\",\"params\":{}}]" > "$work/after.json"

font="fontfile='C\:/Windows/Fonts/arial.ttf'"
for view in side front three-quarter; do
  for run in before after; do
    "$ac" --assets assets video --script "$work/$run.json" --view "$view" --size 1280x720 --fixed \
      --out "$work/$run-$view.mp4"
  done
  "$ffmpeg" -y -i "$work/before-$view.mp4" -i "$work/after-$view.mp4" -filter_complex \
    "[0]drawtext=$font:text='BEFORE (keys, physics off)':x=20:y=20:fontsize=30:fontcolor=white[a];[1]drawtext=$font:text='AFTER (secondary motion, physics off)':x=20:y=20:fontsize=30:fontcolor=white[b];[a][b]hstack" \
    -c:v libx264 -pix_fmt yuv420p "$out/secondary-$view.mp4"
done
# the stop is short: the side view at quarter speed from the last step on (frame 70 at 30 fps)
"$ffmpeg" -y -ss 2.33 -i "$out/secondary-side.mp4" -filter:v "setpts=4*PTS" -c:v libx264 -pix_fmt yuv420p \
  "$out/secondary-side-stop-quarter-speed.mp4"
echo "$out"
