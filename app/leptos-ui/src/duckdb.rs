use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn init_duckdb();

    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn query_duckdb(query: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn insert_json_duckdb(table_name: &str, json_string: &str) -> JsValue;
}
