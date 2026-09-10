#!/usr/bin/env bash
# Drive the MemoryLane TV app on a Vega device with remote key presses and save a screenshot
# of each screen to docs/screens/. Needs the Vega SDK `vega` CLI, a connected device with
# the app installed (`make tv-vvd-install`), and Developer Mode in the device shell. Key
# names follow inputd-cli; the sequence assumes the app is on Onboarding after a fresh
# install and has been approved from the phone before the Home steps run.
#
# Screenshots: gwsi-tool-screenshooter works only on a physical Fire TV (Vega CLI docs). On
# the Virtual Device the script captures the host screen with macOS `screencapture` instead
# (SHOT=host), which needs the Virtual Device window in front.
#
# Unverified on a device: written from the Vega SDK 0.24 documentation.
set -euo pipefail

DEVICE="${VVD:-VirtualDevice}"
OUT="${1:-docs/screens}"
PAUSE="${WALK_PAUSE:-2}"
SHOT="${SHOT:-device}" # device (physical Fire TV) or host (Virtual Device window, macOS)
mkdir -p "$OUT"

shell() { vega exec vda shell "$@"; }
press() { shell inputd-cli button_press "$1"; sleep "$PAUSE"; }
shot() {
  if [ "$SHOT" = "host" ]; then
    screencapture -x "$OUT/$1.png"
  else
    shell gwsi-tool-screenshooter "/tmp/walk-$1.png"
    vega device copy-from --device "$DEVICE" --source "/tmp/walk-$1.png"
    mv "walk-$1.png" "$OUT/$1.png"
  fi
  echo "saved $OUT/$1.png"
}

shell vsm developer-mode enable || true

# 1. Onboarding: code and QR.
shot onboarding

# 2. Home. Waits for the operator to approve the code from the phone.
read -r -p "Approve the code on the phone, then press Enter " _
sleep "$PAUSE"
shot home

# 3. First trip: focus the first tile and open it.
press KEY_DOWN
press KEY_ENTER
shot trip

# 4. Invite from the trip view (first action button).
press KEY_UP
press KEY_RIGHT
press KEY_ENTER
shot invite

# 5. Back to the trip and Home with the hardware Back key.
press KEY_BACK
press KEY_BACK
shot home-after-back
