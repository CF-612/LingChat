//! CosyVoice 云端语音合成适配器（HTTP 非流式）。
//!
//! 官方端点：`POST /api/v1/services/audio/tts/SpeechSynthesizer`，
//! 请求 `{model, input: {text, voice, format, sample_rate}}`，非流式直接返回音频字节。

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

use crate::ai_service::tts::adapters::http_client;
use crate::ai_service::tts::provider::TtsAdapter;

const BASE_URL: &str = "https://dashscope.aliyuncs.com/api/v1";
const SYNTHESIS_PATH: &str = "/services/audio/tts/SpeechSynthesizer";

#[derive(Debug, Clone)]
pub struct CosyvoiceAdapter {
    api_key: String,
    model: String,
    voice_id: String,
}

impl CosyvoiceAdapter {
    pub fn new(api_key: String, model: String, voice_id: String) -> Self {
        Self {
            api_key,
            model,
            voice_id,
        }
    }
}

#[async_trait]
impl TtsAdapter for CosyvoiceAdapter {
    async fn generate_voice(&self, text: &str, _emo: &str) -> Result<Vec<u8>> {
        if text.trim().is_empty() {
            return Err(anyhow!("CosyVoice 输入文本为空"));
        }
        tracing::debug!(
            "CosyVoice synthesize model={} voice={}",
            self.model,
            self.voice_id
        );

        let body = json!({
            "model": self.model,
            "input": {
                "text": text,
                "voice": self.voice_id,
                "format": "wav",
                "sample_rate": 24000,
            }
        });
        let resp = http_client()
            .post(format!("{BASE_URL}{SYNTHESIS_PATH}"))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("CosyVoice 合成失败: HTTP {status}: {text}"));
        }
        Ok(resp.bytes().await?.to_vec())
    }

    fn get_params(&self) -> HashMap<String, JsonValue> {
        let mut m = HashMap::new();
        m.insert("model".into(), json!(self.model));
        m.insert("voice_id".into(), json!(self.voice_id));
        m
    }
}
