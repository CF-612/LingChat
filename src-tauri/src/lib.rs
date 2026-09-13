mod achievements;
mod adventures;
mod ai_service;
mod api;
mod app;
mod cast;
mod config;
mod data_dir;
mod db;
mod lan_sync;
mod manifest;
mod migration;
mod plugins;
mod resource_sync;
pub mod utils;

// 全局状态容器已拆分至 `app::state`，这里重导出以保持 `crate::AppState`
// 等既有路径不变（全仓库 45 个文件、约 200 处引用零改动）。
pub use app::state::{AppState, ChatComponents, InnerAppState, ScreenshotCaptureState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TLS 兜底：rustls 依赖图同时启用 aws-lc-rs（本项目显式）与 ring
    // （tokio-tungstenite rustls-tls-webpki-roots 引入）两个 crypto feature，
    // 进程级默认 provider 无法自动确定 → 走默认 ClientConfig::builder() 的
    // 路径（如 ASR 流式 WebSocket 握手）会 panic。显式安装 aws-lc-rs 为默认。
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // 初始化日志系统，并拿到 genai 调试开关的热重载句柄（setup 阶段使用）。
    // 句柄经 `setup` 的 `FnOnce` 闭包移入 `app::setup::setup`。
    let log_filter = app::logging::init_tracing();

    // 提前构建 Tauri 上下文（读取 bundle identifier，供 Windows HDR 开关定位 settings.json）
    let context = tauri::generate_context!();

    // Windows：设置 WebView2 颜色配置文件（强制使用线性 sRGB）。
    // 非 Windows 上为空实现，因此调用点无需 `#[cfg]`。
    app::platform::apply_webview2_color_profile(&context.config().identifier);

    app::builder::build()
        .setup(move |app| app::setup::setup(app, log_filter))
        .run(context)
        .expect("error while running tauri application");
}
