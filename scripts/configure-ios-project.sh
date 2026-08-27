#!/usr/bin/env bash
# ============================================================================
# Configure LingChat's iOS Xcode project (macOS only).
#
# Responsibilities:
#   1. If src-tauri/gen/apple/ does not exist, run `tauri ios init` to
#      generate the Xcode project (XcodeGen under the hood; keeps project.yml
#      and <app>.xcodeproj).
#   2. Normalize TARGETED_DEVICE_FAMILY = "1,2" (iPhone + iPad).
#      XcodeGen's default is already '1,2'; we force it here to guarantee
#      iPhone & iPad compatibility.
#
# NOTE: tauri-cli does not ship the `ios` subcommand on Windows/Linux, so
# this script only runs on macOS.
# Requirements: Xcode (+ Command Line Tools), xcodegen (`brew install xcodegen`),
# Rust target aarch64-apple-ios (`rustup target add aarch64-apple-ios`).
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

GEN_APPLE="src-tauri/gen/apple"

# --- 1. Initialize the Xcode project ----------------------------------------

if [ ! -d "$GEN_APPLE" ]; then
  echo "[configure-ios] gen/apple missing, running: pnpm tauri ios init --ci"
  if ! command -v xcodegen >/dev/null 2>&1; then
    echo "[configure-ios] ERROR: xcodegen not found. Install it with: brew install xcodegen" >&2
    exit 1
  fi
  pnpm tauri ios init --ci
  echo "[configure-ios] Xcode project generated"
else
  echo "[configure-ios] $GEN_APPLE already exists, skipping init"
fi

PBXPROJ="$(ls "$GEN_APPLE"/*.xcodeproj/project.pbxproj 2>/dev/null | head -1 || true)"
if [ -z "$PBXPROJ" ] || [ ! -f "$PBXPROJ" ]; then
  echo "[configure-ios] ERROR: project.pbxproj not found under $GEN_APPLE" >&2
  exit 1
fi

# --- 2. Normalize TARGETED_DEVICE_FAMILY = "1,2" (iPhone + iPad) -------------

if grep -q "TARGETED_DEVICE_FAMILY" "$PBXPROJ"; then
  # BSD sed (built into macOS): unify every device-family setting to iPhone + iPad
  sed -i '' -E 's/TARGETED_DEVICE_FAMILY = [^;]+;/TARGETED_DEVICE_FAMILY = "1,2";/g' "$PBXPROJ"
  echo "[configure-ios] TARGETED_DEVICE_FAMILY normalized to \"1,2\" (iPhone + iPad)"
else
  echo "[configure-ios] WARNING: TARGETED_DEVICE_FAMILY not found in pbxproj " \
    "(XcodeGen should generate '1,2' by default). Verify in Xcode Build Settings." >&2
fi

COUNT="$(grep -c 'TARGETED_DEVICE_FAMILY = "1,2"' "$PBXPROJ" || true)"
echo "[configure-ios] TARGETED_DEVICE_FAMILY=\"1,2\" occurrences: $COUNT"

# --- 3. Verify the iOS-specific Info.plist merge hook ------------------------

if [ -f "src-tauri/Info.ios.plist" ]; then
  echo "[configure-ios] src-tauri/Info.ios.plist present (Files-app visibility + device family); it is merged into Info.plist on every tauri ios build"
else
  echo "[configure-ios] WARNING: src-tauri/Info.ios.plist missing (UIFileSharingEnabled etc. will be absent)" >&2
fi

echo "[configure-ios] iOS project configured: iPhone + iPad, data dir visible in the Files app"
