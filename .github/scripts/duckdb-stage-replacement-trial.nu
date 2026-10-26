#!/usr/bin/env nu

# Hosted technical trial only. The workflow authenticates the PR117 artifact.
def main [input_dir: string, output_dir: string, receipt_dir: string] {
    let input = ($input_dir | path expand)
    let output = ($output_dir | path expand)
    let receipt = ($receipt_dir | path expand)
    if ($output | path exists) or ($receipt | path exists) {
        error make {msg: "Output and receipt directories must be absent"}
    }
    let expected = [
        {name: "duckdb-browser.mjs", sha256: "8ccf3fe3c6bd87d5c3e397f01d40cc11ea6373b016a9cf5e954d39ee22c3aa49"}
        {name: "duckdb-browser-mvp.worker.js", sha256: "d7920b73954d99ffa59175db35fe24e7936f8a003144386686b53c853c46eaef"}
        {name: "duckdb-browser-eh.worker.js", sha256: "feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4"}
        {name: "duckdb-mvp.wasm", sha256: "7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791"}
        {name: "duckdb-eh.wasm", sha256: "d71b7e4575c98b87e09ac799c574d770aa5ee4df5e6c6cf36b6e277d92b8d2c2"}
    ]
    let files = (glob $"($input)/*" | where {|file| ($file | path type) == "file"})
    if ($files | length) != 5 { error make {msg: "Expected exactly five candidate files"} }
    mkdir $output
    mkdir $receipt
    for asset in $expected {
        let source = $"($input)/($asset.name)"
        let destination = $"($output)/($asset.name)"
        if not ($source | path exists) { error make {msg: $"Missing candidate asset: ($asset.name)"} }
        let actual = (^sha256sum $source | split row " " | first)
        if $actual != $asset.sha256 { error make {msg: $"Candidate hash mismatch: ($asset.name)"} }
        cp $source $destination
        let staged = (^sha256sum $destination | split row " " | first)
        if $staged != $asset.sha256 { error make {msg: $"Staged hash mismatch: ($asset.name)"} }
    }
    {schema: "tundra-duckdb-replacement-trial/v1", source_run: 40217360882, source_head: "7182d3f1b51cf643f73d40f15b576006accddd7d", artifact_id: 12607318747, artifact_digest: "sha256:be8fade74d4dda4b5eae20fc3f906370f6fd9fc82e606810a469b24affa5e7b5", assets: $expected, admission: "NO"} | to json | save $"($receipt)/replacement-trial.json"
}
