#!/usr/bin/env bash
# ============================================================================
# 配置 LingChat 的 iOS Xcode 工程（仅在 macOS 上运行）。
#
# 职责：
#   1. 若 src-tauri/gen/apple/ 不存在，执行 `tauri ios init` 生成 Xcode 工程
#      （内部通过 XcodeGen 生成，产物保留 project.yml 与 <app>.xcodeproj）。
#   2. 断言/归一化 TARGETED_DEVICE_FAMILY = "1,2"（iPhone + iPad 兼容）。
#      XcodeGen 默认即 '1,2'，这里显式兜底，满足「兼容 iPhone 和 iPad」要求。
#
# 注意：Windows/Linux 上 tauri-cli 不提供 ios 子命令，本脚本只能在 macOS 执行。
# 依赖：Xcode（含 Command Line Tools）、xcodegen（brew install xcodegen）、
#       Rust 目标 aarch64-apple-ios（rustup target add aarch64-apple-ios）。
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

GEN_APPLE="src-tauri/gen/apple"

# ─── 1. 初始化 Xcode 工程 ─────────────────────────────────────

if [ ! -d "$GEN_APPLE" ]; then
  echo "🔧 未找到 $GEN_APPLE，执行 tauri ios init ..."
  if ! command -v xcodegen >/dev/null 2>&1; then
    echo "❌ 缺少 xcodegen，请先安装：brew install xcodegen" >&2
    exit 1
  fi
  pnpm tauri ios init --ci
  echo "✅ Xcode 工程已生成"
else
  echo "ℹ️  已存在 $GEN_APPLE，跳过 init"
fi

PBXPROJ="$(ls "$GEN_APPLE"/*.xcodeproj/project.pbxproj 2>/dev/null | head -1 || true)"
if [ -z "$PBXPROJ" ] || [ ! -f "$PBXPROJ" ]; then
  echo "❌ 未找到 project.pbxproj，请检查 $GEN_APPLE 下的 Xcode 工程" >&2
  exit 1
fi

# ─── 2. 归一化 TARGETED_DEVICE_FAMILY = "1,2" ────────────────

if grep -q "TARGETED_DEVICE_FAMILY" "$PBXPROJ"; then
  # BSD sed（macOS 自带）：把所有设备族设置统一为 iPhone + iPad
  sed -i '' -E 's/TARGETED_DEVICE_FAMILY = [^;]+;/TARGETED_DEVICE_FAMILY = "1,2";/g' "$PBXPROJ"
  echo "✅ TARGETED_DEVICE_FAMILY 已归一化为 \"1,2\"（iPhone + iPad）"
else
  echo "⚠  pbxproj 中未找到 TARGETED_DEVICE_FAMILY（XcodeGen 应默认生成 '1,2'），" \
    "请打开 Xcode 工程在 Build Settings 中确认 TARGETED_DEVICE_FAMILY = 1,2" >&2
fi

COUNT="$(grep -c 'TARGETED_DEVICE_FAMILY = "1,2"' "$PBXPROJ" || true)"
echo "   TARGETED_DEVICE_FAMILY=\"1,2\" 出现 $COUNT 处"

# ─── 3. 校验 iOS 专属 Info.plist 合并钩子 ─────────────────────

if [ -f "src-tauri/Info.ios.plist" ]; then
  echo "ℹ️  src-tauri/Info.ios.plist 存在（文件 App 可见 + 设备族），构建时会自动合并进 Info.plist"
else
  echo "⚠  缺少 src-tauri/Info.ios.plist（UIFileSharingEnabled 等键将缺失）" >&2
fi

echo "✅ iOS 工程配置完成：iPhone + iPad 兼容、数据目录对「文件」App 可见"
