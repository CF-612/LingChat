#!/usr/bin/env bash
# ============================================================================
# 在 macOS 上构建 LingChat 的 iOS 无签名 IPA。
#
# 流程：
#   1. configure-ios-project.sh —— 初始化/配置 Xcode 工程（iPhone + iPad）
#   2. prepare-bundled-resources.mjs —— 把默认资源打包成 data.7z 并部署到
#      gen/apple/assets/data/（folder reference 打进 app bundle 根目录）
#   3. pnpm tauri ios build --no-sign —— 前端构建 + Rust 交叉编译 + 产出
#      无签名 IPA（tauri-cli 内建 create_ipa，无需额外打包步骤）
#
# 产物：src-tauri/gen/apple/target/.../release-iphoneos/.../LingChat.ipa
#       可直接用侧载工具（如 Sideloadly / AltStore / 爱思助手）安装到真机，
#       或转交开发者证书签名后分发。
#
# 依赖（仅 macOS）：Xcode、xcodegen、Rust 目标 aarch64-apple-ios、
#       aarch64-apple-ios-sim（模拟器可选）。
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ─── 1. 配置 Xcode 工程 ───────────────────────────────────────
echo "▶ 步骤 1/3：配置 iOS Xcode 工程"
bash scripts/configure-ios-project.sh

# ─── 2. 打包资源 data.7z ──────────────────────────────────────
echo "▶ 步骤 2/3：打包默认资源 data.7z（含 third_party 模型）"
node scripts/prepare-bundled-resources.mjs 9

# ─── 3. 无签名构建 ────────────────────────────────────────────
echo "▶ 步骤 3/3：pnpm tauri ios build --no-sign（产物为无签名 IPA）"
# beforeBuildCommand 会自动执行 prepare-desktop-resources.mjs + pnpm build
pnpm tauri ios build --no-sign "$@"

# ─── 4. 定位产物 ──────────────────────────────────────────────
IPA="$(find src-tauri/gen/apple/target -name "*.ipa" 2>/dev/null | head -1 || true)"
if [ -n "$IPA" ]; then
  echo ""
  echo "📦 无签名 IPA 已生成：$(pwd)/$IPA"
  echo "   （可侧载到 iPhone/iPad：Sideloadly / AltStore / 爱思助手）"
else
  echo "⚠ 未在 src-tauri/gen/apple/target 下找到 .ipa，请检查上方构建日志" >&2
  exit 1
fi
