#!/bin/bash
# Find line number of <div class="task-detail-content">
START_LINE=394
# Find line number where task-detail-content closes
# It's right before the Edit Task Modal dialog
END_LINE=$(grep -n "// Edit Task Modal" app/leptos-ui/src/components/task_detail.rs | cut -d: -f1 | head -n 1)
if [ -z "$END_LINE" ]; then
    echo "Could not find END_LINE"
    exit 1
fi
END_LINE=$((END_LINE - 1))

sed -i '' "${START_LINE}a\\
            <div class=\"td-body\">\\
                <div class=\"td-main\">\\
" app/leptos-ui/src/components/task_detail.rs

# Insert closing tags and sidebar before the end line
sed -i '' "${END_LINE}i\\
                </div>\\
                <div class=\"td-sidebar\">\\
                    <div class=\"td-sidebar-section\">\\
                        <h4>\"Metadata\"</h4>\\
                        <div class=\"td-meta-row\">\\
                            <span class=\"td-meta-label\">\"Status\"</span>\\
                            <span class=\"td-meta-value\">{status_display}</span>\\
                        </div>\\
                        <div class=\"td-meta-row\">\\
                            <span class=\"td-meta-label\">\"Assignee\"</span>\\
                            <span class=\"td-meta-value\">{if agents.is_empty() { \"Unassigned\".to_string() } else { agents.join(\", \") }}</span>\\
                        </div>\\
                        <div class=\"td-meta-row\">\\
                            <span class=\"td-meta-label\">\"Due Date\"</span>\\
                            <span class=\"td-meta-value\">\"None\"</span>\\
                        </div>\\
                    </div>\\
                </div>\\
            </div>\\
" app/leptos-ui/src/components/task_detail.rs

