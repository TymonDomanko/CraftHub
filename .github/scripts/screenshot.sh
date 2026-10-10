#!/usr/bin/env bash
# Open Craft Hub's window for a few seconds and take a screenshot (Windows/macOS runners).
#   screenshot.sh <crafthub binary> <output.png>
set -uo pipefail
BIN="$1"
OUT="$2"

"$BIN" &
pid=$!
sleep 25
case "$RUNNER_OS" in
  Windows)
    powershell -NoProfile -Command "
      Add-Type -AssemblyName System.Windows.Forms, System.Drawing
      \$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
      \$bmp = New-Object System.Drawing.Bitmap \$b.Width, \$b.Height
      \$g = [System.Drawing.Graphics]::FromImage(\$bmp)
      \$g.CopyFromScreen(\$b.Location, [System.Drawing.Point]::Empty, \$b.Size)
      \$bmp.Save('$OUT')"
    taskkill //F //IM "$(basename "$BIN")" >/dev/null 2>&1
    ;;
  macOS)
    screencapture -x "$OUT"
    kill "$pid" 2>/dev/null
    ;;
esac
ls -la "$OUT"
