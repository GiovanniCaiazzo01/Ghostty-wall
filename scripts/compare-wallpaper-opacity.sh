#!/usr/bin/env bash
# Static compositing study, NOT a Ghostty screenshot or renderer verification.
set -euo pipefail
out=${1:?Usage: bash scripts/compare-wallpaper-opacity.sh OUTPUT_DIRECTORY}
mkdir -p "$out"
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for image in light dark; do
  if [[ $image == light ]]; then brightness=150; else brightness=55; fi
  magick "$root/media/welcome.png" -resize '520x240^' -gravity center -extent 520x240 \
    -modulate "$brightness,100,100" "$out/$image.png"
done
panels=()
for theme in dark light; do
  if [[ $theme == dark ]]; then bg='#101820'; fg='#e8edf2'; else bg='#eeeeee'; fg='#182028'; fi
  for image in light dark; do
    for opacity in 0.1 0.075 0.05; do
      panel="$work/$theme-$image-$opacity.png"
      magick -size 520x240 "xc:$bg" \
        \( "$out/$image.png" -alpha set -channel A -evaluate multiply "$opacity" +channel \) \
        -compose over -composite -font DejaVu-Sans-Mono -pointsize 16 -fill "$fg" \
        -gravity northwest -annotate +18+20 "$theme background / $image image / $opacity" \
        -annotate +18+65 '$ cargo test --locked' \
        -annotate +18+100 'running 42 tests' \
        -annotate +18+135 'test profile_default ... ok' \
        -annotate +18+185 'Normal text 0123456789 [] {} ()' "$panel"
      panels+=("$panel")
    done
  done
done
magick montage "${panels[@]}" -tile 3x4 -geometry +6+6 -background '#808080' "$out/comparison.png"
printf '%s\n' "Static comparison: $out/comparison.png (not live Ghostty proof)"
