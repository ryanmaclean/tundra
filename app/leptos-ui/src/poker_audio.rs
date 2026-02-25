use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = pokerAudio)]
    async fn warmup() -> JsValue;

    #[wasm_bindgen(js_namespace = pokerAudio)]
    async fn play_cue(cue_name: &str) -> JsValue;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioCueResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub cue: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

fn parse_audio_result(raw: JsValue) -> Result<AudioCueResult, String> {
    let json = raw
        .as_string()
        .unwrap_or_else(|| js_sys::JSON::stringify(&raw).map(|s| s.into()).unwrap_or_default());

    serde_json::from_str::<AudioCueResult>(&json)
        .map_err(|e| format!("audio cue parse error: {e}"))
}

pub async fn init_audio() -> Result<AudioCueResult, String> {
    parse_audio_result(warmup().await)
}

pub async fn cue(name: &str) -> Result<AudioCueResult, String> {
    parse_audio_result(play_cue(name).await)
}
