#!/usr/bin/env nu

# Stage only the five authenticated PR117 replacement assets. The caller checks
# the artifact's workflow head, ID, digest, and retained provenance receipts.
def main [output_dir: string, receipt_dir: string, input_dir: string] {
    let bill = (open app/tauri/third-party-licenses/ui/duckdb-runtime-selected-bill.json)
    if $bill.schema != "tundra-duckdb-selected-notice-bill/v2" or $bill.admission != "NO" or ($bill.assets | length) != 5 {
        error make {msg: "Unexpected replacement DuckDB asset bill"}
    }
    if ($output_dir | path exists) { error make {msg: "DuckDB output directory must be absent"} }
    let input_files = (glob $"($input_dir)/*" | where {|file| ($file | path type) == "file"})
    if ($input_files | length) != 5 { error make {msg: "Expected exactly five source assets"} }
    mkdir $output_dir
    mkdir $receipt_dir
    for asset in $bill.assets {
        let name = ($asset.path | path basename)
        if $asset.path != $"duckdb-runtime/($name)" or not ($name in [duckdb-browser.mjs duckdb-browser-mvp.worker.js duckdb-browser-eh.worker.js duckdb-mvp.wasm duckdb-eh.wasm]) {
            error make {msg: $"Unexpected DuckDB asset path: ($asset.path)"}
        }
        let source = $"($input_dir)/($name)"
        let actual = (^sha256sum $source | str trim | split row ' ' | first)
        if $env.LAST_EXIT_CODE != 0 or $actual != $asset.sha256 { error make {msg: $"Source DuckDB asset mismatch: ($name)"} }
        cp $source $"($output_dir)/($name)"
        let staged = (^sha256sum $"($output_dir)/($name)" | str trim | split row ' ' | first)
        if $env.LAST_EXIT_CODE != 0 or $staged != $asset.sha256 { error make {msg: $"Staged DuckDB asset mismatch: ($name)"} }
    }
    let files = (glob $"($output_dir)/*" | sort)
    if ($files | length) != 5 { error make {msg: "DuckDB output file set mismatch"} }
    ^sha256sum ...$files | save --force $"($receipt_dir)/duckdb-generated-assets.sha256"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB asset receipt failed"} }
}
