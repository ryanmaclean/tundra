use leptos::prelude::*;

/// Animated SVG loading spinner with purple→cyan gradient arc.
/// Sizes: "sm" (16px), "md" (24px), "lg" (32px)
#[component]
pub fn Spinner(
    #[prop(default = "md")] size: &'static str,
    #[prop(default = "")] label: &'static str,
) -> impl IntoView {
    let (w, h) = match size {
        "sm" => (16, 16),
        "lg" => (32, 32),
        _ => (24, 24),
    };
    let svg = format!(
        r##"<svg width="{w}" height="{h}" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" class="svg-spinner">
  <defs>
    <linearGradient id="spinGrad-{w}" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#7c3aed"/>
      <stop offset="100%" stop-color="#06b6d4"/>
    </linearGradient>
  </defs>
  <circle cx="12" cy="12" r="10" fill="none" stroke="rgba(124,58,237,0.12)" stroke-width="2.5"/>
  <path d="M12 2a10 10 0 0 1 10 10" fill="none" stroke="url(#spinGrad-{w})" stroke-width="2.5" stroke-linecap="round">
    <animateTransform attributeName="transform" type="rotate" from="0 12 12" to="360 12 12" dur="0.9s" repeatCount="indefinite"/>
  </path>
  <circle cx="22" cy="12" r="1.5" fill="#06b6d4" opacity="0.6">
    <animateTransform attributeName="transform" type="rotate" from="0 12 12" to="360 12 12" dur="0.9s" repeatCount="indefinite"/>
    <animate attributeName="opacity" values="0.6;0.2;0.6" dur="0.9s" repeatCount="indefinite"/>
  </circle>
</svg>"##,
        w = w,
        h = h,
    );
    let size_class = format!("spinner-container spinner-container-{}", size);
    view! {
        <div class={size_class}>
            <span class="svg-spinner-wrap" inner_html=svg></span>
            {(!label.is_empty()).then(|| view! {
                <span class="spinner-label">{label}</span>
            })}
        </div>
    }
}
