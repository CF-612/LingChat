//! CosyVoice 云端音色服务：注册（含本地文件上传）/查询/列表/删除。

pub mod commands;
pub mod enrollment;
#[cfg(test)]
pub mod enrollment_test;
pub mod upload;

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::Future;

use crate::config::tts::CosyVoiceRecord;

pub use enrollment::*;
pub use upload::*;

/// 音色名 sanitize 成 ASCII 字母数字前缀（官方要求仅数字字母 ≤10 字符）；
/// 中文等不可转写字符会被滤掉，全滤空时回退 "voice"。
/// voice_id 格式：{target_model}-{prefix}-{唯一标识}。
fn sanitize_prefix(name: &str) -> String {
    let out: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect::<String>()
        .to_lowercase();
    if out.is_empty() {
        "voice".to_string()
    } else {
        out
    }
}

/// 云端音色服务：注册（自动上传/URL 兜底）+ 轮询 + 列表 + 删除。
#[derive(Debug, Clone)]
pub struct CloudVoiceService {
    api_key: String,
}

impl CloudVoiceService {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// 本地文件注册：自动上传 OSS → create_voice → 轮询 → CosyVoiceRecord。
    pub async fn create_from_file(
        &self,
        model: &str,
        name: &str,
        file_path: &Path,
        progress: impl Fn(&str),
    ) -> Result<CosyVoiceRecord> {
        let prefix = sanitize_prefix(name);
        progress("上传语音样本中…");
        let url = upload_audio(&self.api_key, model, file_path).await?;
        progress("提交复刻任务…");
        tracing::info!(
            "CosyVoice 创建音色: model={} prefix={} name={}",
            model,
            prefix,
            name
        );
        let voice_id = create_voice(&self.api_key, model, &prefix, &url, Some(&["zh"])).await?;
        progress("音色处理中（约需几十秒）…");
        poll_until_ready(
            &mut |voice_id: String| async move {
                let status = query_voice(&self.api_key, &voice_id).await?;
                tracing::debug!("CosyVoice 音色状态轮询: {voice_id} -> {status}");
                Ok(status)
            },
            voice_id.clone(),
            30,
            Duration::from_secs(10),
        )
        .await?;
        tracing::info!("CosyVoice 音色就绪: {}", voice_id);
        Ok(CosyVoiceRecord {
            voice_id,
            name: name.to_string(),
            model: model.to_string(),
            created_at: Some(unix_seconds_str()),
        })
    }

    /// 公网 URL 注册（兜底路径）。
    pub async fn create_from_url(&self, model: &str, name: &str, url: &str) -> Result<CosyVoiceRecord> {
        let prefix = sanitize_prefix(name);
        tracing::info!(
            "CosyVoice 创建音色(URL): model={} prefix={} name={}",
            model,
            prefix,
            name
        );
        let voice_id = create_voice(&self.api_key, model, &prefix, url, Some(&["zh"])).await?;
        poll_until_ready(
            &mut |voice_id: String| async move {
                let status = query_voice(&self.api_key, &voice_id).await?;
                tracing::debug!("CosyVoice 音色状态轮询: {voice_id} -> {status}");
                Ok(status)
            },
            voice_id.clone(),
            30,
            Duration::from_secs(10),
        )
        .await?;
        tracing::info!("CosyVoice 音色就绪: {}", voice_id);
        Ok(CosyVoiceRecord {
            voice_id,
            name: name.to_string(),
            model: model.to_string(),
            created_at: Some(unix_seconds_str()),
        })
    }

    /// 云端音色列表（带状态）。不带 prefix 过滤——音色名各异，全部拉取再合并本地映射。
    pub async fn list(&self) -> Result<Vec<VoiceListItem>> {
        let ids = list_voices(&self.api_key, None).await?;
        // 逐个查询状态（数量通常个位数）；查询失败不致命，标记为 None
        let mut items = Vec::new();
        for id in ids {
            let status = query_voice(&self.api_key, &id).await.ok();
            items.push(VoiceListItem { voice_id: id, status });
        }
        Ok(items)
    }

    pub async fn delete(&self, voice_id: &str) -> Result<()> {
        delete_voice(&self.api_key, voice_id).await
    }
}

#[derive(Debug, Clone)]
pub struct VoiceListItem {
    pub voice_id: String,
    pub status: Option<String>,
}

fn unix_seconds_str() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// 轮询音色处理状态直到 OK/UNDEPLOYED/超时（纯逻辑，查询函数依赖注入以便测试）。
/// `query` 接收 `String`（拥有）避免闭包返回的 future 借用参数导致 lifetime 泛型问题。
pub async fn poll_until_ready<F, Fut>(
    query: &mut F,
    voice_id: String,
    max_attempts: u32,
    interval: Duration,
) -> Result<()>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    for attempt in 1..=max_attempts {
        let status = query(voice_id.clone()).await?;
        match status.as_str() {
            "OK" => return Ok(()),
            "UNDEPLOYED" => {
                return Err(anyhow!(
                    "音色处理失败（UNDEPLOYED），请检查音频质量或联系支持"
                ))
            }
            _ => {
                if attempt < max_attempts {
                    tokio::time::sleep(interval).await;
                }
            }
        }
    }
    Err(anyhow!(
        "音色处理超时（已等待约 {}s），请稍后重试",
        (max_attempts as u64) * interval.as_secs()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn poll_ready_on_ok() {
        let mut query = |_id: String| Box::pin(async { Ok("OK".to_string()) });
        let result =
            poll_until_ready(&mut query, "v1".into(), 30, std::time::Duration::from_millis(1))
                .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn poll_fails_on_undeployed() {
        let mut query = |_id: String| Box::pin(async { Ok("UNDEPLOYED".to_string()) });
        let result =
            poll_until_ready(&mut query, "v1".into(), 30, std::time::Duration::from_millis(1))
                .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UNDEPLOYED"));
    }

    #[tokio::test]
    async fn poll_times_out() {
        let mut query = |_id: String| Box::pin(async { Ok("PROCESSING".to_string()) });
        let result =
            poll_until_ready(&mut query, "v1".into(), 3, std::time::Duration::from_millis(1))
                .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("超时"));
    }

    #[tokio::test]
    async fn poll_stops_after_two_processing() {
        let mut calls = 0;
        let mut query = move |_id: String| {
            calls += 1;
            Box::pin(async move {
                if calls >= 2 {
                    Ok("OK".to_string())
                } else {
                    Ok("PROCESSING".to_string())
                }
            })
        };
        let result =
            poll_until_ready(&mut query, "v1".into(), 30, std::time::Duration::from_millis(1))
                .await;
        assert!(result.is_ok());
    }

    #[test]
    fn sanitize_prefix_ascii_alnum_lowercased() {
        assert_eq!(sanitize_prefix("NuoYi123"), "nuoyi123");
    }

    #[test]
    fn sanitize_prefix_truncates_at_10() {
        assert_eq!(sanitize_prefix("abcdefghijklmnop"), "abcdefghij");
    }

    #[test]
    fn sanitize_prefix_filters_non_ascii() {
        assert_eq!(sanitize_prefix("诺一_One"), "one");
        assert_eq!(sanitize_prefix("诺一"), "voice");
        assert_eq!(sanitize_prefix(""), "voice");
    }
}
