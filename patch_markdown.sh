#!/bin/bash
sed -i '' '/use leptos::task::spawn_local;/a\
use pulldown_cmark::{Parser, html};\
' app/leptos-ui/src/components/task_detail.rs

sed -i '' '/let \(all_pass, checks, _suggestions\) = qa_report.get();/a\
                    let mut html_output = String::new();\
                    let parser = Parser::new(&desc);\
                    html::push_html(&mut html_output, parser);\
' app/leptos-ui/src/components/task_detail.rs

sed -i '' 's/<p class="td-spec-text">{d}<\/p>/<div class="td-spec-text markdown-body" inner_html=html_output><\/div>/g' app/leptos-ui/src/components/task_detail.rs
