#!/usr/bin/env bash
# Launch the Love2D client with a usable window.
#
# Under i3, bare `love frontend/love` often tiles into a ~200px strip that looks
# like "nothing happened." This script builds shipsim if needed, starts Love,
# then floats/resizes the window (retries until height is usable).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if ! command -v love >/dev/null 2>&1; then
  echo "error: love (Love2D 11.x) not found on PATH" >&2
  exit 1
fi

if [[ ! -x target/debug/shipsim && ! -x target/release/shipsim ]]; then
  echo "building shipsim..."
  cargo build -q
fi

export DISPLAY="${DISPLAY:-:0}"
export SHIPSIM_ROOT="${SHIPSIM_ROOT:-$ROOT}"

love frontend/love "$@" &
LOVE_PID=$!

float_once() {
  local wid="${1:-}"
  if [[ -n "$wid" ]]; then
    i3-msg "[id=$wid] floating enable, border pixel 2, resize set 1280 800, move position center, focus" \
      >/dev/null 2>&1 && return 0
  fi
  i3-msg '[class="^love$"] floating enable, border pixel 2, resize set 1280 800, move position center, focus' \
    >/dev/null 2>&1 || true
}

window_height() {
  local wid="$1"
  if command -v xdotool >/dev/null 2>&1; then
    xdotool getwindowgeometry "$wid" 2>/dev/null | awk '/Geometry:/{split($2,a,"x"); print a[2]+0}'
  else
    echo 0
  fi
}

find_love_wid() {
  if ! command -v xdotool >/dev/null 2>&1; then
    echo ""
    return 0
  fi
  local wid
  wid="$(xdotool search --class love 2>/dev/null | tail -1 || true)"
  if [[ -z "$wid" ]]; then
    wid="$(xdotool search --name '^shipsim$' 2>/dev/null | tail -1 || true)"
  fi
  echo "$wid"
}

float_window() {
  if ! command -v i3-msg >/dev/null 2>&1; then
    return 0
  fi

  # Retry: Love may map late; keep re-floating until height is playable.
  local tries=0
  local wid h
  while (( tries < 60 )); do
    wid="$(find_love_wid)"
    if [[ -n "$wid" ]]; then
      float_once "$wid"
      sleep 0.08
      h="$(window_height "$wid")"
      if [[ "${h:-0}" -ge 600 ]]; then
        return 0
      fi
    else
      float_once ""
    fi
    sleep 0.05
    tries=$((tries + 1))
  done
  return 0
}

float_window

echo "shipsim Love2D running (pid $LOVE_PID)"
echo "  picker: Up/Down + Enter · Exit/Q/Esc quits · help: ? or H"
if command -v i3-msg >/dev/null 2>&1; then
  h=0
  wid="$(find_love_wid)"
  if [[ -n "$wid" ]]; then
    h="$(window_height "$wid")"
  fi
  if [[ "${h:-0}" -ge 600 ]]; then
    echo "  i3: floated (~${h}px tall)"
  else
    echo "  i3: still tiled/short — press \$mod+Shift+Space then resize, or:"
    echo "    i3-msg '[class=\"^love$\"] floating enable, resize set 1280 800, move position center'"
  fi
fi

wait "$LOVE_PID" || true
