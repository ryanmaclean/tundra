use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = webgpuAnalytics)]
    async fn run_probe_webgpu(workgroups: u32) -> JsValue;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebGpuProbeReport {
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub elapsed_ms: Option<f64>,
    #[serde(default)]
    pub sample: Vec<u32>,
    #[serde(default)]
    pub workgroups: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn probe(workgroups: u32) -> Result<WebGpuProbeReport, String> {
    let raw = run_probe_webgpu(workgroups).await;
    let json = raw.as_string().unwrap_or_else(|| {
        js_sys::JSON::stringify(&raw)
            .map(|s| s.into())
            .unwrap_or_default()
    });

    serde_json::from_str::<WebGpuProbeReport>(&json)
        .map_err(|e| format!("webgpu probe parse error: {e}"))
}
