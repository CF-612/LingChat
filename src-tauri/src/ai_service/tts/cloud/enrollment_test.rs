use serde_json::json;

use crate::ai_service::tts::cloud::enrollment::{
    create_voice_body, parse_voice_id, parse_voice_list, parse_voice_status,
};

#[test]
fn create_voice_body_shape() {
    let body = create_voice_body(
        "cosyvoice-v3.5-flash",
        "myvoice",
        "oss://bucket/a.wav",
        Some(&["zh"]),
    );
    assert_eq!(body["model"], "voice-enrollment");
    assert_eq!(body["input"]["action"], "create_voice");
    assert_eq!(body["input"]["target_model"], "cosyvoice-v3.5-flash");
    assert_eq!(body["input"]["prefix"], "myvoice");
    assert_eq!(body["input"]["url"], "oss://bucket/a.wav");
    assert_eq!(body["input"]["language_hints"][0], "zh");
}

#[test]
fn create_voice_body_default_language_hints() {
    let body = create_voice_body("cosyvoice-v3.5-flash", "myvoice", "https://x.com/a.wav", None);
    assert_eq!(body["input"]["language_hints"][0], "zh");
}

#[test]
fn parse_voice_id_ok() {
    let v = json!({"output": {"voice_id": "cosyvoice-v3.5-flash-myvoice-abc123"}});
    assert_eq!(
        parse_voice_id(&v).unwrap(),
        "cosyvoice-v3.5-flash-myvoice-abc123"
    );
}

#[test]
fn parse_voice_id_missing() {
    let v = json!({"output": {}});
    assert!(parse_voice_id(&v).is_err());
}

#[test]
fn parse_voice_status_ok() {
    let v = json!({"output": {"status": "OK"}});
    assert_eq!(parse_voice_status(&v).unwrap(), "OK");
}

#[test]
fn parse_voice_list_response() {
    let v = json!({"output": {"voice_list": [{"voice_id": "a"}, {"voice_id": "b"}]}});
    let ids = parse_voice_list(&v).unwrap();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}
