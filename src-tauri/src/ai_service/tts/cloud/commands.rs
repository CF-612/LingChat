//! CosyVoice 相关 Tauri commands。

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::CloudVoiceService;
use crate::config::keys;
use crate::config::tts::{CosyVoiceRecord, TtsConfig};
use crate::ai_service::tts::provider::TtsAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosyvoiceConfig {
    pub api_key_configured: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosyVoiceView {
    pub voice_id: String,
    pub name: String,
    pub model: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosyvoiceProgress {
    pub phase: String,
}

fn service(app: &AppHandle) -> Result<CloudVoiceService> {
    let cfg = TtsConfig::load(app);
    let key = cfg.cosyvoice_api_key.unwrap_or_default();
    if key.trim().is_empty() {
        return Err(anyhow::anyhow!("CosyVoice API Key 未配置，请在设置中填写"));
    }
    Ok(CloudVoiceService::new(key))
}

fn read_models(app: &AppHandle) -> Vec<String> {
    TtsConfig::load(app).cosyvoice_models
}

fn write_models(app: &AppHandle, models: Vec<String>) -> Result<()> {
    let store = crate::config::settings_store(app)?;
    store.set(keys::COSYVOICE_MODELS, serde_json::json!(models));
    store.save()?;
    Ok(())
}

fn read_voice_records(app: &AppHandle) -> Vec<CosyVoiceRecord> {
    TtsConfig::load(app).cosyvoice_voices
}

fn write_voice_records(app: &AppHandle, records: Vec<CosyVoiceRecord>) -> Result<()> {
    let store = crate::config::settings_store(app)?;
    store.set(keys::COSYVOICE_VOICES, serde_json::json!(records));
    store.save()?;
    Ok(())
}
#[tauri::command]
pub async fn cosyvoice_get_config(app: AppHandle) -> Result<CosyvoiceConfig, String> {
    let cfg = TtsConfig::load(&app);
    Ok(CosyvoiceConfig {
        api_key_configured: cfg
            .cosyvoice_api_key
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false),
        models: cfg.cosyvoice_models,
    })
}

#[tauri::command]
pub async fn cosyvoice_save_api_key(app: AppHandle, api_key: String) -> Result<(), String> {
    let store = crate::config::settings_store(&app).map_err(|e| e.to_string())?;
    store.set(keys::COSYVOICE_API_KEY, serde_json::json!(api_key));
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cosyvoice_add_model(app: AppHandle, model: String) -> Result<(), String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("模型名不能为空".into());
    }
    let mut models = read_models(&app);
    if !models.contains(&model) {
        models.push(model);
        write_models(&app, models).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cosyvoice_remove_model(app: AppHandle, model: String) -> Result<(), String> {
    let mut models = read_models(&app);
    models.retain(|m| *m != model);
    write_models(&app, models).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cosyvoice_create_voice(
    app: AppHandle,
    name: String,
    model: String,
    file_path: String,
    channel: tauri::ipc::Channel<CosyvoiceProgress>,
) -> Result<CosyVoiceRecord, String> {
    // 上传大小限制 20MB（与参考实现一致）
    const MAX_SAMPLE_BYTES: u64 = 20 * 1024 * 1024;
    let path = PathBuf::from(file_path);
    let meta = std::fs::metadata(&path).map_err(|e| format!("读取语音样本失败: {e}"))?;
    if meta.len() > MAX_SAMPLE_BYTES {
        return Err(format!(
            "语音样本超过 20MB 限制（当前 {:.1}MB）",
            meta.len() as f64 / (1024.0 * 1024.0)
        ));
    }
    let svc = service(&app).map_err(|e| e.to_string())?;
    let record = svc
        .create_from_file(&model, &name, &path, &|phase: &str| {
            let _ = channel.send(CosyvoiceProgress {
                phase: phase.to_string(),
            });
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut records = read_voice_records(&app);
    records.retain(|r| r.voice_id != record.voice_id);
    records.push(record.clone());
    write_voice_records(&app, records).map_err(|e| e.to_string())?;
    Ok(record)
}

#[tauri::command]
pub async fn cosyvoice_create_voice_from_url(
    app: AppHandle,
    name: String,
    model: String,
    url: String,
) -> Result<CosyVoiceRecord, String> {
    let svc = service(&app).map_err(|e| e.to_string())?;
    let record = svc
        .create_from_url(&model, &name, &url)
        .await
        .map_err(|e| e.to_string())?;
    let mut records = read_voice_records(&app);
    records.retain(|r| r.voice_id != record.voice_id);
    records.push(record.clone());
    write_voice_records(&app, records).map_err(|e| e.to_string())?;
    Ok(record)
}

#[tauri::command]
pub async fn cosyvoice_list_voices(app: AppHandle) -> Result<Vec<CosyVoiceView>, String> {
    let svc = match service(&app) {
        Ok(s) => s,
        // 未配置 Key：返回空列表，由前端提示
        Err(_) => return Ok(Vec::new()),
    };
    let cloud = svc.list().await.unwrap_or_default();
    let records = read_voice_records(&app);
    let mut views = Vec::new();
    for item in cloud {
        let record = records.iter().find(|r| r.voice_id == item.voice_id);
        views.push(CosyVoiceView {
            voice_id: item.voice_id.clone(),
            name: record
                .map(|r| r.name.clone())
                .unwrap_or_else(|| item.voice_id.clone()),
            model: record.map(|r| r.model.clone()).unwrap_or_default(),
            status: item.status,
        });
    }
    Ok(views)
}

#[tauri::command]
pub async fn cosyvoice_delete_voice(app: AppHandle, voice_id: String) -> Result<(), String> {
    // 云端删除失败不阻断本地移除
    if let Ok(svc) = service(&app) {
        let _ = svc.delete(&voice_id).await;
    }
    let mut records = read_voice_records(&app);
    records.retain(|r| r.voice_id != voice_id);
    write_voice_records(&app, records).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cosyvoice_synthesize_preview(
    app: AppHandle,
    model: String,
    voice_id: String,
    text: String,
) -> Result<Vec<u8>, String> {
    let svc = service(&app).map_err(|e| e.to_string())?;
    // 复用 adapter 的合成逻辑（与对话链路同构）
    let adapter =
        crate::ai_service::tts::adapters::cosyvoice::CosyvoiceAdapter::new(
            svc.api_key().to_string(),
            model,
            voice_id,
        );
    adapter
        .generate_voice(&text, "")
        .await
        .map_err(|e| e.to_string())
}
