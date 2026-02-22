import re

let_file = "app/leptos-ui/style.css"
with open(let_file, "r") as f:
    content = f.read()

# Find all occurrences of .new-task-modal and .edit-task-modal to prevent duplication
# For all block selectors starting with .new-task-modal
lines = content.split('\n')
new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    if '.new-task-modal' in line and '{' in line and '.edit-task-modal' not in line:
        line = line.replace('.new-task-modal', '.new-task-modal, .edit-task-modal')
    elif '.new-task-modal' in line and ',' in line and '{' not in line and '.edit-task-modal' not in line:
        # A selector list like .new-task-modal h2,
        line = line.replace('.new-task-modal', '.new-task-modal, .edit-task-modal')
    elif line.strip().startswith('.new-task-modal') and '{' not in line and '.edit-task-modal' not in line:
        # Single line selector before { on next line
        new_lines.append(line)
        line = line.replace('.new-task-modal', '.edit-task-modal')
        
    new_lines.append(line)
    i += 1

with open(let_file, "w") as f:
    f.write('\n'.join(new_lines))
