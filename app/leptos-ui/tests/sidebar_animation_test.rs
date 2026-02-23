#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use web_sys::window;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

fn element_exists(selector: &str) -> bool {
    window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector(selector).ok().flatten())
        .is_some()
}

#[wasm_bindgen]
pub fn run_sidebar_animation_tests() {
    if element_exists(".sidebar") {
        log("PASS: sidebar exists");
    } else {
        log("FAIL: sidebar missing");
    }

    if element_exists(".sidebar-toggle-btn") {
        log("PASS: sidebar toggle exists");
    } else {
        log("FAIL: sidebar toggle missing");
    }
}

#[wasm_bindgen]
pub fn init_animation_tests() {
    run_sidebar_animation_tests();
}
