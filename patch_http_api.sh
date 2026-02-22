#!/bin/bash
sed -i '' '/pub description: Option<String>,/a\
    pub tags: Option<Vec<String>>,
' crates/at-bridge/src/http_api.rs

sed -i '' '/if let Some(description) = req.description {/a\
    }\
    if let Some(tags) = req.tags {\
        bead.tags = tags;
' crates/at-bridge/src/http_api.rs
