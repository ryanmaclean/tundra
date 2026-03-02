use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

/// Creates a focus trap that cycles Tab/Shift+Tab through focusable elements
/// within a container element.
///
/// Returns a closure that can be used with `on:keydown` event handler.
///
/// # Example
/// ```rust,ignore
/// let handle_keydown = use_focus_trap();
/// view! {
///     <div on:keydown=handle_keydown>
///         <button>"First"</button>
///         <button>"Second"</button>
///     </div>
/// }
/// ```
pub fn use_focus_trap() -> impl Fn(leptos::ev::KeyboardEvent) {
    move |ev: leptos::ev::KeyboardEvent| {
        // Only handle Tab key
        if ev.key() != "Tab" {
            return;
        }

        // Get the current target element (the container with the focus trap)
        let Some(current_target) = ev.current_target() else {
            return;
        };

        let Ok(container) = current_target.dyn_into::<HtmlElement>() else {
            return;
        };

        // Query all focusable elements within the container
        let focusable_elements = get_focusable_elements(&container);

        if focusable_elements.is_empty() {
            return;
        }

        // Get currently focused element
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("window should have a document");

        let Some(active_element) = document.active_element() else {
            return;
        };

        // Find the index of currently focused element
        let current_index = focusable_elements
            .iter()
            .position(|el| el.is_same_node(Some(&active_element)));

        let Some(current_index) = current_index else {
            // If no focusable element is currently focused within the container,
            // focus the first one on Tab
            if !ev.shift_key() {
                if let Some(first) = focusable_elements.first() {
                    let _ = first.focus();
                    ev.prevent_default();
                }
            }
            return;
        };

        // Calculate next index based on Shift+Tab or Tab
        let next_index = if ev.shift_key() {
            // Shift+Tab: go backwards, wrap to last if at first
            if current_index == 0 {
                focusable_elements.len() - 1
            } else {
                current_index - 1
            }
        } else {
            // Tab: go forwards, wrap to first if at last
            if current_index >= focusable_elements.len() - 1 {
                0
            } else {
                current_index + 1
            }
        };

        // Focus the next element and prevent default tab behavior
        if let Some(next_element) = focusable_elements.get(next_index) {
            let _ = next_element.focus();
            ev.prevent_default();
        }
    }
}

/// Queries all focusable elements within a container.
/// Includes: a, button, input, select, textarea, and elements with tabindex >= 0
fn get_focusable_elements(container: &HtmlElement) -> Vec<HtmlElement> {
    let selector = r#"a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])"#;

    // Use JavaScript directly to call querySelectorAll on the container
    let js_container: &JsValue = container.as_ref();
    let query_fn = js_sys::eval("(function(s) { return this.querySelectorAll(s); })").unwrap();
    let query_fn = query_fn.dyn_ref::<js_sys::Function>().unwrap();

    let node_list = js_sys::Reflect::apply(
        query_fn,
        js_container,
        &js_sys::Array::of1(&JsValue::from_str(selector)),
    );

    let Ok(node_list) = node_list else {
        return Vec::new();
    };

    // Convert NodeList to Array for easier iteration
    let array = js_sys::Array::from(&node_list);
    let mut elements = Vec::new();

    for i in 0..array.length() {
        if let Some(item) = array.get(i).dyn_ref::<HtmlElement>() {
            elements.push(item.clone());
        }
    }

    elements
}
