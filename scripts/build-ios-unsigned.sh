#!/usr/bin/env bash
# ============================================================================
# Build LingChat's iOS unsigned IPA (macOS only).
#
# Pipeline:
#   1. configure-ios-project.sh -- init/configure the Xcode project (iPhone + iPad)
#   2. prepare-bundled-resources.mjs -- pack default resources into data.7z and
#      deploy to gen/apple/assets/data/ (folder reference -> app bundle root)
#   3. pnpm tauri ios build --no-sign -- build frontend + cross-compile Rust and
#      produce an UNSIGNED IPA (tauri-cli has a built-in create_ipa step)
#
# Output: src-tauri/gen/apple/target/**/*.ipa
#   Install via sideloading tools (Sideloadly / AltStore / 3uTools) or sign it
#   with a developer certificate for distribution.
#
# Requirements (macOS only): Xcode, xcodegen, Rust target aarch64-apple-ios
# (aarch64-apple-ios-sim optional for the simulator).
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# --- 1. Configure the Xcode project ------------------------------------------
echo "[build-ios] Step 1/3: configuring the iOS Xcode project"
bash scripts/configure-ios-project.sh

# --- 2. Pack resources into data.7z ------------------------------------------
echo "[build-ios] Step 2/3: packing default resources into data.7z (incl. third_party models)"
node scripts/prepare-bundled-resources.mjs 9

# --- 3. Unsigned build --------------------------------------------------------
echo "[build-ios] Step 3/3: pnpm tauri ios build --no-sign (unsigned IPA)"
# beforeBuildCommand automatically runs prepare-desktop-resources.mjs + pnpm build
pnpm tauri ios build --no-sign "$@"

# --- 4. Locate the artifact ---------------------------------------------------
# cargo-mobile2 把产物放在 gen/apple/build/<arch>/ 下（如 build/arm64/LingChat.ipa）；
# 兼容旧目录 gen/apple/target 一并查找
IPA="$(find src-tauri/gen/apple/build src-tauri/gen/apple/target -name "*.ipa" 2>/dev/null | head -1 || true)"
if [ -n "$IPA" ]; then
  echo ""
  echo "[build-ios] unsigned IPA ready: $(pwd)/$IPA"
  echo "[build-ios] sideload to iPhone/iPad with Sideloadly / AltStore / 3uTools"
else
  echo "[build-ios] ERROR: no .ipa found under src-tauri/gen/apple/build or target, check the build log above" >&2
  exit 1
fi
