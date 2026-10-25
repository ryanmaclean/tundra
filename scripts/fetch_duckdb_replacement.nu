#!/usr/bin/env nu

# Fetch and authenticate the exact retained PR117 replacement artifact.
# This is an expiring draft input; admission remains NO until source gaps close.
def main [output_dir: string, receipt_dir: string] {
    if ($output_dir | path exists) { error make {msg: "Artifact output directory must be absent"} }
    if not ("GH_TOKEN" in $env) { error make {msg: "Missing GitHub Actions token"} }
    let bill = (open app/tauri/third-party-licenses/ui/duckdb-runtime-selected-bill.json)
    let source = $bill.asset_source
    if $source.repo != "ryanmaclean/tundra" or $source.head != "7182d3f1b51cf643f73d40f15b576006accddd7d" or $source.run_id != 40217360882 or $source.artifact_id != 12607318747 or $source.artifact_digest != "sha256:be8fade74d4dda4b5eae20fc3f906370f6fd9fc82e606810a469b24affa5e7b5" {
        error make {msg: "Replacement artifact identity changed"}
    }
    mkdir $receipt_dir
    let metadata = $"($receipt_dir)/duckdb-artifact-metadata.json"
    let zip = $"($receipt_dir)/duckdb-replacement.zip"
    let url = $"https://api.github.com/repos/($source.repo)/actions/artifacts/($source.artifact_id)"
    ^curl --fail --silent --show-error --location --retry 3 --header $"Authorization: Bearer ($env.GH_TOKEN)" --header "Accept: application/vnd.github+json" $url -o $metadata
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB artifact metadata fetch failed"} }
    let meta = (open $metadata)
    if $meta.id != $source.artifact_id or $meta.digest != $source.artifact_digest or $meta.expired != false or $meta.workflow_run.id != $source.run_id or $meta.workflow_run.head_sha != $source.head or $meta.name != "duckdb-matching-browser-candidate" {
        error make {msg: "DuckDB artifact metadata mismatch or expired"}
    }
    ^curl --fail --silent --show-error --location --retry 3 --header $"Authorization: Bearer ($env.GH_TOKEN)" --header "Accept: application/vnd.github+json" $"($url)/zip" -o $zip
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB artifact download failed"} }
    let hash = (^sha256sum $zip | str trim | split row ' ' | first)
    if $env.LAST_EXIT_CODE != 0 or $"sha256:($hash)" != $source.artifact_digest { error make {msg: "DuckDB artifact ZIP mismatch"} }
    ^unzip -tq $zip
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB artifact ZIP corrupt"} }
    mkdir $output_dir
    ^unzip -q $zip -d $output_dir
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB artifact extraction failed"} }
    let heads = (open $"($output_dir)/input-receipt/heads.txt")
    for expected in [
        "tundra_trial_head=7182d3f1b51cf643f73d40f15b576006accddd7d"
        "source_head=f955138e5218942fb3fb097a8fdcb725222406ff"
        "native_tundra_head=a7d43e055ad33f73bb7cd4333b80ebd70ea940ea"
        "native_run=40216203853"
    ] {
        if not ($heads | str contains $expected) { error make {msg: $"Missing DuckDB artifact head receipt: ($expected)"} }
    }
    let candidate = (open $"($output_dir)/matching-receipt/candidate.json")
    if $candidate.admission != "NO" or $candidate.coi_built != false or $candidate.source_commit != "f955138e5218942fb3fb097a8fdcb725222406ff" {
        error make {msg: "DuckDB candidate receipt mismatch"}
    }
}
