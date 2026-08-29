//! 本地音频上传到 DashScope 临时存储（48h 有效），换取 oss:// 临时 URL。
//!
//! 流程：POST /api/v1/uploads?action=getPolicy 取凭证 → multipart 直传 OSS → 拼 oss://key。
//! 注意：getPolicy 响应字段名以实际为准；实现遇解析失败时打印完整响应便于排查。

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::ai_service::tts::adapters::http_client;

const BASE_URL: &str = "https://dashscope.aliyuncs.com/api/v1";
const UPLOADS_PATH: &str = "/uploads";

/// getPolicy 响应中的字段（字段名以实际响应为准，此处为官方示例中的常见字段）。
struct UploadPolicy {
    host: String,
    upload_dir: String,
    policy: String,
    access_id: String,
    signature: String,
}

fn parse_policy(data: &Value) -> Result<UploadPolicy> {
    Ok(UploadPolicy {
        host: data["host"]
            .as_str()
            .unwrap_or("https://oss-cn-beijing.aliyuncs.com")
            .to_string(),
        upload_dir: data["upload_dir"]
            .as_str()
            .ok_or_else(|| anyhow!("getPolicy 缺少 upload_dir: {data}"))?
            .to_string(),
        policy: data["policy"]
            .as_str()
            .ok_or_else(|| anyhow!("getPolicy 缺少 policy: {data}"))?
            .to_string(),
        access_id: data["access_id"]
            .as_str()
            .ok_or_else(|| anyhow!("getPolicy 缺少 access_id: {data}"))?
            .to_string(),
        signature: data["signature"]
            .as_str()
            .ok_or_else(|| anyhow!("getPolicy 缺少 signature: {data}"))?
            .to_string(),
    })
}

/// 上传本地音频，返回 oss:// 形式临时 URL。
pub async fn upload_audio(api_key: &str, model: &str, file_path: &Path) -> Result<String> {
    let resp = http_client()
        .post(format!("{BASE_URL}{UPLOADS_PATH}"))
        .query(&[("action", "getPolicy"), ("model", model)])
        .bearer_auth(api_key)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("获取上传凭证失败: HTTP {status}: {text}"));
    }
    let v: Value = resp.json().await?;
    let policy = parse_policy(&v["data"])?;

    let file_name = file_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sample.wav".to_string());
    let key = format!("{}/{}", policy.upload_dir, file_name);

    let bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| anyhow!("读取音频文件失败: {e}"))?;

    let form = reqwest::multipart::Form::new()
        .text("key", key.clone())
        .text("policy", policy.policy.clone())
        .text("OSSAccessKeyId", policy.access_id.clone())
        .text("signature", policy.signature.clone())
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes)
                .file_name(file_name)
                .mime_str("application/octet-stream")?,
        );

    let upload_resp = http_client()
        .post(policy.host.clone())
        .multipart(form)
        .send()
        .await?;
    if !upload_resp.status().is_success() {
        let status = upload_resp.status();
        let text = upload_resp.text().await.unwrap_or_default();
        return Err(anyhow!("OSS 直传失败: HTTP {status}: {text}"));
    }

    Ok(format!("oss://{}", key))
}
