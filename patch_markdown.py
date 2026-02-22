import re

with open('app/leptos-ui/src/components/task_detail.rs', 'r') as f:
    content = f.read()

# 1. Add import if missing
if 'use pulldown_cmark' not in content:
    content = content.replace(
        'use leptos::task::spawn_local;',
        'use leptos::task::spawn_local;\nuse pulldown_cmark::{Parser, html};'
    )

# 2. Inject html_output definition before view!
target = r'''let \(all_pass, checks, _suggestions\) = qa_report\.get\(\);
                    view! \{'''

replacement = r'''let (all_pass, checks, _suggestions) = qa_report.get();
                    let mut html_output = String::new();
                    let parser = Parser::new(&desc);
                    html::push_html(&mut html_output, parser);
                    view! {'''

content = re.sub(target, replacement, content)

# 3. Replace <p class="td-spec-text">{d}</p> with inner_html=html_output
content = content.replace(
    '<p class="td-spec-text">{d}</p>',
    '<div class="td-spec-text markdown-body" inner_html={html_output.clone()}></div>'
)

with open('app/leptos-ui/src/components/task_detail.rs', 'w') as f:
    f.write(content)

