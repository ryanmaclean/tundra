#!/usr/bin/env nu

# Fetch pinned diagnostic byte custody. The original Actions producer remains
# the producer; this commit is not a new build or product admission.
def digest [file: string] {
    let hash = (^sha256sum $file | str trim | split row ' ' | first)
    if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Hash failed: ($file)"} }
    $hash
}

def verify_file [root: string, name: string, expected: record] {
    let file = $"($root)/($name)"
    if ($file | path type) != "file" or (digest $file) != $expected.sha256 or (ls $file | get size.0 | into int) != $expected.size {
        error make {msg: $"Custody byte mismatch: ($name)"}
    }
    let blob = (^git hash-object $file | str trim)
    if $env.LAST_EXIT_CODE != 0 or $blob != $expected.git_blob { error make {msg: $"Custody Git blob mismatch: ($name)"} }
}

def main [output_dir: string, receipt_dir: string] {
    let commit = "1d77df7776dca44731ae8267092e480a78f9f735"
    let repo = "ryanmaclean/tundra"
    let bill_path = "app/tauri/third-party-licenses/ui/duckdb-runtime-selected-bill.json"
    if ($output_dir | path exists) or ($receipt_dir | path exists) { error make {msg: "DuckDB fetch directories must be absent"} }
    let bill = (open $bill_path)
    let source = $bill.asset_source
    if $bill.schema != "tundra-duckdb-selected-notice-bill/v2" or $bill.admission != "NO" or ($bill.assets | length) != 5 or ($bill.texts | length) != 36 or ($bill.attributions | length) != 38 {
        error make {msg: "Unexpected DuckDB selected bill"}
    }
    if $source.repo != $repo or $source.head != "7182d3f1b51cf643f73d40f15b576006accddd7d" or $source.run_id != 40217360882 or $source.artifact_id != 12607318747 or $source.artifact_digest != "sha256:be8fade74d4dda4b5eae20fc3f906370f6fd9fc82e606810a469b24affa5e7b5" {
        error make {msg: "Replacement producer identity changed"}
    }
    if (digest $bill_path) != "34381cba60c8922a159f578753b55531460792372f2d72b22769da205fe97790" { error make {msg: "Selected bill bytes changed"} }

    mkdir $receipt_dir
    let custody = $"($receipt_dir)/custody-checkout"
    mkdir $custody
    ^git -C $custody init --quiet
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody Git init failed"} }
    ^git -C $custody remote add origin $"https://github.com/($repo).git"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody Git remote failed"} }
    ^git -C $custody fetch --quiet --depth 1 origin refs/heads/custody/duckdb-browser-assets-40217360882
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody Git fetch failed"} }
    let fetched = (^git -C $custody rev-parse FETCH_HEAD | str trim)
    if $env.LAST_EXIT_CODE != 0 or $fetched != $commit { error make {msg: "Custody commit identity changed"} }

    let manifest_raw = (^git -C $custody show $"($commit):custody/manifest.json")
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody manifest read failed"} }
    let manifest = ($manifest_raw | from json)
    if $manifest.schema != "tundra-diagnostic-browser-asset-custody/v1" or $manifest.admission != "NO" or $manifest.purpose != "Diagnostic byte custody only; not a release, product, source-build, or legal admission" {
        error make {msg: "Custody admission changed"}
    }
    if $manifest.artifact.repo != $repo or $manifest.artifact.producer_head != $source.head or $manifest.artifact.run_id != $source.run_id or $manifest.artifact.artifact_id != $source.artifact_id or $"sha256:($manifest.artifact.zip_sha256)" != $source.artifact_digest or $manifest.artifact.zip_entries != 35 or $manifest.artifact.zip_crc != "all_passed" {
        error make {msg: "Custody producer identity changed"}
    }
    if $manifest.native_inputs.duckdb_wasm_source_head != "f955138e5218942fb3fb097a8fdcb725222406ff" or $manifest.native_inputs.tundra_native_head != "a7d43e055ad33f73bb7cd4333b80ebd70ea940ea" or $manifest.native_inputs.native_run_id != 40216203853 {
        error make {msg: "Custody native producer identity changed"}
    }
    if ($manifest.assets | columns | length) != 5 or ($manifest.receipts | columns | length) != 6 or ($manifest.legal.texts | columns | length) != 36 or $manifest.legal.text_count != 36 or $manifest.legal.attribution_count != 38 or $manifest.legal.bill_sha256 != (digest $bill_path) or $manifest.legal.source_head != "1ccac98a3cfe7a7b1e28b900761d81b94fa5f64f" {
        error make {msg: "Custody content manifest changed"}
    }
    let expected = (["custody/manifest.json" $manifest.legal.bill_path] | append ($manifest.assets | columns) | append ($manifest.receipts | columns) | append ($manifest.legal.texts | columns) | flatten | sort)
    let tree = (^git -C $custody ls-tree -r $commit | lines)
    if $env.LAST_EXIT_CODE != 0 or ($tree | length) != 49 or ($tree | any {|entry| not ($entry | str starts-with "100644 blob ")}) { error make {msg: "Custody tree has missing or non-regular files"} }
    let actual = ($tree | each {|entry| $entry | split row (char tab) | last} | sort)
    if $actual != $expected { error make {msg: "Custody tree path allowlist mismatch"} }
    ^git -C $custody checkout --quiet --detach $commit
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody checkout failed"} }
    let checked = (^git -C $custody rev-parse HEAD | str trim)
    if $env.LAST_EXIT_CODE != 0 or $checked != $commit { error make {msg: "Custody checkout identity changed"} }

    let custody_bill = $"($custody)/($manifest.legal.bill_path)"
    if (digest $custody_bill) != $manifest.legal.bill_sha256 or (^git hash-object $custody_bill | str trim) != $manifest.legal.bill_git_blob { error make {msg: "Custody bill mismatch"} }
    ^cmp $bill_path $custody_bill
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Custody bill differs from tracked bill"} }
    let legal_names = ($bill.texts | get file | sort)
    if $legal_names != ($manifest.legal.texts | columns | sort) { error make {msg: "Custody legal set differs from selected bill"} }
    for entry in ($manifest.legal.texts | transpose name value) {
        verify_file $custody $entry.name $entry.value
        let tracked = $"app/tauri/($entry.name)"
        if ($tracked | path type) != "file" or (digest $tracked) != $entry.value.sha256 { error make {msg: $"Tracked legal text differs: ($entry.name)"} }
        ^cmp $tracked $"($custody)/($entry.name)"
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Custody legal text differs: ($entry.name)"} }
    }
    let joins = ($bill.attributions | each {|entry| $"($entry.surface)|($entry.component)|($entry.source_url)|($entry.file)"})
    if ($joins | uniq | length) != 38 { error make {msg: "Duplicate selected attribution"} }
    for attribution in $bill.attributions {
        let texts = ($bill.texts | where file == $attribution.file)
        if ($texts | length) != 1 or $texts.0.sha256 != $attribution.source_sha256 or not ($attribution.file in $legal_names) { error make {msg: $"Attribution lacks legal text: ($attribution.component)"} }
    }

    let asset_names = ([duckdb-browser.mjs duckdb-browser-mvp.worker.js duckdb-browser-eh.worker.js duckdb-mvp.wasm duckdb-eh.wasm] | sort)
    if ($bill.assets | each {|entry| $entry.path | path basename} | sort) != $asset_names { error make {msg: "Selected asset names changed"} }
    for entry in ($manifest.assets | transpose name value) {
        verify_file $custody $entry.name $entry.value
        let matches = ($bill.assets | where path == $entry.name)
        if ($matches | length) != 1 or $matches.0.sha256 != $entry.value.sha256 or $entry.value.zip_entry != $"matching-output/($entry.name | path basename)" { error make {msg: $"Custody asset differs from bill: ($entry.name)"} }
    }
    for entry in ($manifest.receipts | transpose name value) { verify_file $custody $entry.name $entry.value }
    let heads = (open --raw $"($custody)/input-receipt/heads.txt")
    for line in [
        "tundra_trial_head=7182d3f1b51cf643f73d40f15b576006accddd7d"
        "source_head=f955138e5218942fb3fb097a8fdcb725222406ff"
        "native_tundra_head=a7d43e055ad33f73bb7cd4333b80ebd70ea940ea"
        "native_run=40216203853"
    ] {
        if not ($line in ($heads | lines)) { error make {msg: $"Missing original producer receipt: ($line)"} }
    }
    let candidate = (open $"($custody)/matching-receipt/candidate.json")
    if $candidate.admission != "NO" or $candidate.coi_built != false or $candidate.source_commit != $manifest.native_inputs.duckdb_wasm_source_head or $candidate.native_wasm_mvp_sha256 != $manifest.assets."duckdb-runtime/duckdb-mvp.wasm".sha256 or $candidate.native_wasm_eh_sha256 != $manifest.assets."duckdb-runtime/duckdb-eh.wasm".sha256 { error make {msg: "Candidate producer receipt changed"} }
    let five = (open --raw $"($custody)/matching-receipt/five-output.sha256" | lines)
    if ($five | length) != 5 { error make {msg: "Five-output receipt count changed"} }
    for entry in ($manifest.assets | transpose name value) {
        if not ($five | any {|line| $line == $"($entry.value.sha256)  /home/runner/work/_temp/matching-output/($entry.name | path basename)"}) { error make {msg: $"Five-output receipt mismatch: ($entry.name)"} }
    }
    for kind in [browser worker-mvp worker-eh] {
        let metafile = (open $"($custody)/matching-receipt/($kind)-metafile.json")
        let imports = ($metafile.outputs | values | each {|entry| $entry.imports} | flatten)
        if $kind == "browser" {
            if ($imports | length) != 0 { error make {msg: "Browser has external imports"} }
        } else if $imports != [{path: crypto, kind: require-call, external: true}] { error make {msg: $"Worker external import shape changed: ($kind)"} }
    }

    mkdir $output_dir
    mkdir $"($output_dir)/matching-output"
    mkdir $"($output_dir)/matching-receipt"
    mkdir $"($output_dir)/input-receipt"
    for entry in ($manifest.assets | transpose name value) {
        let name = ($entry.name | path basename)
        cp $"($custody)/($entry.name)" $"($output_dir)/matching-output/($name)"
        if (digest $"($output_dir)/matching-output/($name)") != $entry.value.sha256 { error make {msg: $"Staged asset mismatch: ($name)"} }
    }
    for entry in ($manifest.receipts | transpose name value) {
        cp $"($custody)/($entry.name)" $"($output_dir)/($entry.name)"
        if (digest $"($output_dir)/($entry.name)") != $entry.value.sha256 { error make {msg: $"Staged receipt mismatch: ($entry.name)"} }
    }
    {custody_commit: $commit, manifest_sha256: (digest $"($custody)/custody/manifest.json"), producer_run: $source.run_id, producer_head: $source.head, artifact_id: $source.artifact_id, original_artifact_digest: $source.artifact_digest, admission: "NO"} | to json | save $"($receipt_dir)/custody.json"
}
