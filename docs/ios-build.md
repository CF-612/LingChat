# iOS 构建指南（无签名 IPA）

> 本文档描述 LingChat 的 iOS 支持现状与打包流程。
> **iOS 构建只能在 macOS 上执行**（`tauri ios` 子命令仅存在于 macOS 版 tauri-cli）。

## 现状

- 后端（Rust）已支持 iOS：数据播种走 `data.7z`（与 Android 同一机制，见
  `src-tauri/src/init/static_copy.rs` 的 `seed_via_fs_plugin`）。
- iOS 数据目录 = 沙盒内 **Documents**（`<container>/Documents`），配合
  `src-tauri/Info.ios.plist` 的 `UIFileSharingEnabled` / `LSSupportsOpeningDocumentsInPlace`，
  用户可在系统「文件」App 中直接看到并访问整个 `data/` 目录
  （游戏数据、语音、截图、数据库等）。
- Xcode 工程由 `tauri ios init` 生成（XcodeGen），目标**兼容 iPhone 与 iPad**
  （`TARGETED_DEVICE_FAMILY = "1,2"`，XcodeGen 默认即此值，`scripts/configure-ios-project.sh`
  会显式归一化兜底）。
- 截图插件（`tauri-plugin-screenshots`）依赖的 `xcap` 不支持 iOS，已在插件内打桩：
  iOS 构建时排除 xcap，所有截图命令返回「不支持」错误（`screenshots:default`
  权限仍可正常解析，capabilities 无需改动）。

## 前置条件（macOS）

```bash
brew install xcodegen
rustup target add aarch64-apple-ios      # 真机目标
rustup target add aarch64-apple-ios-sim  # 模拟器（可选，本流程默认只打真机包）
pnpm install
pnpm run init        # 生成图标 + 下载情绪模型（ONNX）到 data/third_party
```

## 打包无签名 IPA

### 一键脚本

```bash
pnpm run ios:build
```

等价于（分步）：

```bash
bash scripts/configure-ios-project.sh      # 1. init Xcode 工程 + iPhone/iPad 兜底
node scripts/prepare-bundled-resources.mjs 9  # 2. data.7z → gen/apple/assets/data/
pnpm tauri ios build --no-sign             # 3. 构建（beforeBuildCommand 自动跑前端构建）
```

产物：`src-tauri/gen/apple/target/**/*.ipa`（无签名）。

### CI

`.github/workflows/build-ios.yml`（手动触发）在 `macos-latest` 上执行同一流程，
IPA 作为 workflow artifact 上传（保留 7 天）。

## 侧载安装

无签名 IPA 无法直接双击安装，需要用侧载工具：

- **Sideloadly** / **AltStore**（免费，Apple ID 签名后安装，7 天有效）
- **爱思助手 / 3uTools**（无签名直装，需越狱或开发者模式）
- 企业签名 / TestFlight（需开发者账号）

安装后首次启动需在 设置 → 通用 → VPN 与设备管理 中信任该开发者。

## 关键设计决策

### 1. 为什么数据目录放 Documents 而不是默认的 app_data_dir

- iOS 的 `app_data_dir()` 位于 `Library/Application Support/`，「文件」App 不可见；
- `UIFileSharingEnabled` 只暴露沙盒的 **Documents** 目录；
- 因此 `static_copy.rs` 的 iOS 分支使用 `document_dir()`。

> 注意：Documents 默认会参与 **iCloud 备份**。若担心 `third_party/` 模型（数百 MB）
> 占用备份空间，可后续在启动时对 `data/third_party` 设置 `NSURLIsExcludedFromBackupKey`
> 排除备份（不影响「文件」App 可见性）。

### 2. 为什么 Info.ios.plist 是单独的 plist

`tauri ios build` 每次构建都会**重新合并** Info.plist，来源按序为：
`gen/apple/<app>_iOS/Info.plist`（XcodeGen 生成）→ 版本号 → `src-tauri/Info.plist`（可选）
→ **`src-tauri/Info.ios.plist`（可选，iOS 专属钩子）** → `bundle.ios.info_plist`（配置）。
因此把文件共享键与设备族写进 `Info.ios.plist` 可稳定生效，不随工程再生成而丢失。

### 3. 资源文件怎么进包

| 内容 | 机制 | 落点 |
|---|---|---|
| 游戏数据 + 模型 | `prepare-bundled-resources.mjs` 打包 `data.7z` → `gen/apple/assets/data/data.7z` | `<bundle>/data/data.7z`，首启解压到 Documents |
| 桌面 `.official` 资源 | `prepare-desktop-resources.mjs` 在移动端（`TAURI_ENV_PLATFORM=ios/android`）**只生成空占位** | 不进包（避免与 data.7z 重复） |

`gen/apple/assets/` 在 project.yml 中是 folder reference（`type: folder`），构建时整体
拷入 bundle 根目录，因此 `assets/data/data.7z` → `<bundle>/data/data.7z`，与 Rust 读取路径
`{resource_dir}/data/data.7z` 一致。

> 兼容性说明：`prepare-desktop-resources.mjs` 的 `isMobile` 跳过逻辑对 Android 同样生效，
> Android APK 也会因此去掉重复打包的 `.official` 文件（移动端播种本就只认 data.7z）。

## 已知限制

- 无签名 IPA 只能侧载自用，**不能上架 App Store / TestFlight**；
- Windows / Linux 无法生成或构建 iOS 工程（tauri-cli 未提供 ios 子命令）；
- 本仓库在 Windows 侧无法做 iOS 交叉编译验证，首次真机构建请以 macOS 上
  `pnpm run ios:build` 的实际结果为准；
- ort（ONNX Runtime）在 iOS 有官方预编译产物（`aarch64-apple-ios`），
  已确认可编译链接，无需额外配置。
