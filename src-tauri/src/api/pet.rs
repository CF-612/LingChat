use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};
#[cfg(desktop)]
use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Deserialize, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct HitTestState {
    pub solid_rects: Arc<Mutex<Vec<Rect>>>,
    pub enabled: Arc<Mutex<bool>>,
}

impl Default for HitTestState {
    fn default() -> Self {
        Self {
            solid_rects: Arc::new(Mutex::new(Vec::new())),
            enabled: Arc::new(Mutex::new(false)),
        }
    }
}

/// 当前活动窗口标签（"main" / "pet"）。
///
/// 穿透轮询、存档截图等"操作当前窗口"的逻辑都从这里取 label，
/// 而不是硬编码 "main"。
pub struct ActiveWindow {
    pub label: Arc<Mutex<String>>,
}

impl Default for ActiveWindow {
    fn default() -> Self {
        Self {
            label: Arc::new(Mutex::new("main".into())),
        }
    }
}

/// 桌宠模式切换事件负载（emit 到 main 窗口）。
#[derive(Clone, serde::Serialize)]
pub struct PetModeChangedPayload {
    pub active: bool,
}

/// 桌宠独立窗口的 label。
const PET_WINDOW: &str = "pet";

#[tauri::command]
pub fn update_solid_regions(rects: Vec<Rect>, state: State<'_, HitTestState>) {
    if let Ok(mut locked) = state.solid_rects.lock() {
        *locked = rects;
    }
}

/// 进入桌宠模式：创建/显示独立 pet 窗口并隐藏 main。
///
/// scale 决定桌宠窗口尺寸（基于 BASE_AVATAR_SIZE=240、DIALOG_MAX_BASE=200、
/// CHAT_BASE_H=45：宽 240*scale，高 485*scale）。position 为上次保存的窗口
/// 物理坐标（可选），用于免闪烁地恢复到上次位置。
///
/// 移动端（Android/iOS）无独立窗口概念，只更新穿透状态，保持原占位行为。
#[tauri::command]
#[cfg_attr(not(desktop), allow(unused_variables))]
pub fn enter_pet(
    scale: Option<f64>,
    position: Option<(f64, f64)>,
    app: AppHandle,
    state: State<'_, HitTestState>,
    active_window: State<'_, ActiveWindow>,
) -> Result<(), String> {
    if let Ok(mut locked_enabled) = state.enabled.lock() {
        *locked_enabled = true;
    }

    #[cfg(desktop)]
    {
        let scale_val = scale.unwrap_or(1.0);
        let width = 240.0 * scale_val;
        let height = 485.0 * scale_val;

        if let Some(win) = app.get_webview_window(PET_WINDOW) {
            // 防御性路径：正常流程 pet 已 close，这里兜底重建尺寸并聚焦
            let _ = win.set_size(tauri::LogicalSize::new(width, height));
            let _ = win.show();
            let _ = win.set_focus();
        } else {
            let mut builder = WebviewWindowBuilder::new(
                &app,
                PET_WINDOW,
                WebviewUrl::App("index.html?window=pet".into()),
            )
            .title("LingChat 桌宠")
            .inner_size(width, height)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .focused(true);

            // transparent 仅 Windows 支持（参照 screenshot.rs 的覆盖窗口）
            #[cfg(target_os = "windows")]
            {
                builder = builder.transparent(true);
            }

            // 恢复上次位置，避免窗口在屏幕角落闪一下再跳过去
            if let Some((x, y)) = position {
                builder = builder.position(x, y);
            }

            let app_for_event = app.clone();
            let win = builder
                .build()
                .map_err(|e| format!("创建桌宠窗口失败: {}", e))?;
            // 桌宠被系统关闭（非 exit_pet 主动）时，恢复主窗口并通知前端
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    let _ = app_for_event.emit_to(
                        "main",
                        "pet-mode-changed",
                        PetModeChangedPayload { active: false },
                    );
                    if let Some(main) = app_for_event.get_webview_window("main") {
                        let _ = main.show();
                    }
                }
            });
        }

        // 隐藏主窗口（保留进程，聊天状态留在内存，由后端存档兜底）
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.hide();
        }

        let _ = app.emit_to(
            "main",
            "pet-mode-changed",
            PetModeChangedPayload { active: true },
        );
    }

    if let Ok(mut label) = active_window.label.lock() {
        *label = PET_WINDOW.to_string();
    }
    Ok(())
}

/// 退出桌宠模式：关闭 pet 窗口并恢复 main。
///
/// 顺序：先通知 main 恢复（前端重载后端状态），再关闭 pet（触发 Destroyed
/// 广播，幂等），最后 show/focus main。位置持久化由前端 onMoved 负责，此处不处理。
#[tauri::command]
#[cfg_attr(not(desktop), allow(unused_variables))]
pub fn exit_pet(
    app: AppHandle,
    state: State<'_, HitTestState>,
    active_window: State<'_, ActiveWindow>,
) -> Result<(), String> {
    if let Ok(mut locked_enabled) = state.enabled.lock() {
        *locked_enabled = false;
    }
    if let Ok(mut rects) = state.solid_rects.lock() {
        rects.clear();
    }

    #[cfg(desktop)]
    {
        let _ = app.emit_to(
            "main",
            "pet-mode-changed",
            PetModeChangedPayload { active: false },
        );

        if let Some(pet) = app.get_webview_window(PET_WINDOW) {
            let _ = pet.close();
        }
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.show();
            let _ = main.set_focus();
        }
    }

    if let Ok(mut label) = active_window.label.lock() {
        *label = "main".to_string();
    }
    Ok(())
}

/// 强制落盘一次退出存档（桌宠切回聊天前调用，保证 pet 期间对话不丢）。
#[tauri::command]
pub async fn force_auto_save(app: AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let mut mgr = state.auto_save_manager.lock().await;
    mgr.perform_exit_save().await
}
