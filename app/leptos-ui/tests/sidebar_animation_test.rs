use leptos::prelude::*;
use leptos_dom::*;
use wasm_bindgen::prelude::*;
use web_sys::window;
use std::time::Duration;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Test suite for sidebar animation functionality
pub fn run_sidebar_animation_tests() {
    log("Starting sidebar animation tests...");
    
    // Test 1: Check if sidebar element exists
    test_sidebar_element_exists();
    
    // Test 2: Check toggle button exists
    test_toggle_button_exists();
    
    // Test 3: Test initial state (expanded)
    test_initial_expanded_state();
    
    // Test 4: Test collapse functionality
    test_collapse_functionality();
    
    // Test 5: Test expand functionality
    test_expand_functionality();
    
    // Test 6: Test transition timing
    test_transition_timing();
    
    // Test 7: Test responsive behavior
    test_responsive_behavior();
    
    log("Sidebar animation tests completed!");
}

fn test_sidebar_element_exists() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    match document.query_selector(".sidebar") {
        Some(_) => log("✅ PASS: Sidebar element found"),
        None => log("❌ FAIL: Sidebar element not found"),
    }
}

fn test_toggle_button_exists() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    match document.query_selector(".sidebar-toggle-btn") {
        Some(_) => log("✅ PASS: Toggle button found"),
        None => log("❌ FAIL: Toggle button not found"),
    }
}

fn test_initial_expanded_state() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(sidebar) = document.query_selector(".sidebar") {
        let class_list = sidebar.class_list();
        let is_collapsed = class_list.contains("collapsed");
        
        if is_collapsed {
            log("❌ FAIL: Sidebar should start expanded, but found collapsed");
        } else {
            log("✅ PASS: Sidebar starts in expanded state");
        }
    } else {
        log("❌ FAIL: Could not find sidebar to check initial state");
    }
}

fn test_collapse_functionality() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(toggle_btn) = document.query_selector(".sidebar-toggle-btn") {
        // Simulate click
        let event = document.create_event("click").unwrap();
        toggle_btn.dispatch_event(&event).unwrap();
        
        // Wait a moment for transition to start
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback(
                Box::new(move || {
                    if let Some(sidebar) = document.query_selector(".sidebar") {
                        let class_list = sidebar.class_list();
                        let is_collapsed = class_list.contains("collapsed");
                        
                        if is_collapsed {
                            log("✅ PASS: Sidebar collapses when toggle button clicked");
                        } else {
                            log("❌ FAIL: Sidebar did not collapse when toggle button clicked");
                        }
                    } else {
                        log("❌ FAIL: Could not find sidebar after collapse attempt");
                    }
                }),
                Duration::from_millis(100),
            )
            .unwrap();
    } else {
        log("❌ FAIL: Could not find toggle button to test collapse");
    }
}

fn test_expand_functionality() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(toggle_btn) = document.query_selector(".sidebar-toggle-btn") {
        // Simulate click to expand
        let event = document.create_event("click").unwrap();
        toggle_btn.dispatch_event(&event).unwrap();
        
        // Wait a moment for transition to start
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback(
                Box::new(move || {
                    if let Some(sidebar) = document.query_selector(".sidebar") {
                        let class_list = sidebar.class_list();
                        let is_collapsed = class_list.contains("collapsed");
                        
                        if !is_collapsed {
                            log("✅ PASS: Sidebar expands when toggle button clicked again");
                        } else {
                            log("❌ FAIL: Sidebar did not expand when toggle button clicked again");
                        }
                    } else {
                        log("❌ FAIL: Could not find sidebar after expand attempt");
                    }
                }),
                Duration::from_millis(100),
            )
            .unwrap();
    } else {
        log("❌ FAIL: Could not find toggle button to test expand");
    }
}

fn test_transition_timing() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(sidebar) = document.query_selector(".sidebar") {
        let computed_style = window
            .get_computed_style(&sidebar)
            .unwrap();
        
        let transition_duration = computed_style.get_property_value("transition-duration");
        let transition_timing = computed_style.get_property_value("transition-timing-function");
        
        log(&format!("📊 Transition duration: {}", transition_duration));
        log(&format!("📊 Transition timing: {}", transition_timing));
        
        // Check if transition duration is reasonable (should be around 0.25s)
        if transition_duration.contains("0.25s") || transition_duration.contains("0.2s") {
            log("✅ PASS: Transition duration is appropriate");
        } else {
            log("⚠️  WARN: Transition duration may not be optimal");
        }
        
        // Check if timing function is cubic-bezier
        if transition_timing.contains("cubic-bezier") {
            log("✅ PASS: Using cubic-bezier timing function");
        } else {
            log("⚠️  WARN: Not using cubic-bezier timing function");
        }
    } else {
        log("❌ FAIL: Could not find sidebar to check transition timing");
    }
}

fn test_responsive_behavior() {
    let window = window().unwrap();
    let document = window.document().unwrap();
    
    if let Some(sidebar) = document.query_selector(".sidebar") {
        let computed_style = window
            .get_computed_style(&sidebar)
            .unwrap();
        
        let width = computed_style.get_property_value("width");
        let min_width = computed_style.get_property_value("min-width");
        
        log(&format!("📊 Current width: {}", width));
        log(&format!("📊 Min width: {}", min_width));
        
        // Check if width values are set
        if !width.is_empty() && !min_width.is_empty() {
            log("✅ PASS: Sidebar has width and min-width properties");
        } else {
            log("❌ FAIL: Sidebar missing width properties");
        }
    } else {
        log("❌ FAIL: Could not find sidebar to check responsive behavior");
    }
}

// Export for use in main application
#[wasm_bindgen]
pub fn init_animation_tests() {
    // Set up test runner
    web_sys::window()
        .unwrap()
        .set_timeout_with_callback(
            Box::new(|| {
                run_sidebar_animation_tests();
            }),
            Duration::from_millis(2000), // Wait 2 seconds for page to load
        )
        .unwrap();
}
