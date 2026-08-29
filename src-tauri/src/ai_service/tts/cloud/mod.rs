//! CosyVoice 云端音色服务：注册（含本地文件上传）/查询/列表/删除。

pub mod enrollment;
#[cfg(test)]
pub mod enrollment_test;
pub mod upload;

/// 注册音色统一使用的前缀（官方要求仅数字字母 ≤10 字符）。
/// voice_id 格式：{target_model}-{prefix}-{唯一标识}。
pub const VOICE_PREFIX: &str = "myvoice";
