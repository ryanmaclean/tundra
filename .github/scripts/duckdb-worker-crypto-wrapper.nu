#!/usr/bin/env nu

# Hosted diagnostic preparation only. Does not edit the installed workers.
def main [profile: string, installed_elf: string, runtime_dir: string, expected_elf_sha256: string, output_dir: string] {
    if $nu.os-info.name != "linux" { error make {msg: "Hosted Linux required"} }
    if $profile not-in [default sounds] { error make {msg: "Unknown installed profile"} }
    if ($expected_elf_sha256 | str length) != 64 or not ($expected_elf_sha256 =~ '^[0-9a-f]{64}$') {
        error make {msg: "Expected ELF SHA-256 must be lowercase hex"}
    }
    if ($output_dir | path exists) { error make {msg: "Output directory must not exist"} }
    let elf = ($installed_elf | path expand)
    let runtime = ($runtime_dir | path expand)
    let template = ($env.FILE_PWD | path join "duckdb-worker-crypto-wrapper.template.js")
    if not ($elf | path exists) or not ($template | path exists) { error make {msg: "Missing ELF or wrapper template"} }
    let actual_elf = (^sha256sum $elf | split row ' ' | first)
    if $env.LAST_EXIT_CODE != 0 or $actual_elf != $expected_elf_sha256 { error make {msg: "Installed ELF hash mismatch"} }
    let admitted_elf = if $profile == "default" {
        "e2057adb2a757b804b33024aea0ea7db025e5564c90fa67637674f14014511c3"
    } else {
        "c38acb09084c3bfb5be0157a8106baa2a22aa7883afc22ef20b3fb40a8814025"
    }
    if $actual_elf != $admitted_elf { error make {msg: "ELF not in pinned PR119 installed receipt"} }
    let expected_assets = {
        "duckdb-browser.mjs": "8ccf3fe3c6bd87d5c3e397f01d40cc11ea6373b016a9cf5e954d39ee22c3aa49"
        "duckdb-browser-mvp.worker.js": "d7920b73954d99ffa59175db35fe24e7936f8a003144386686b53c853c46eaef"
        "duckdb-browser-eh.worker.js": "feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4"
        "duckdb-mvp.wasm": "7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791"
        "duckdb-eh.wasm": "d71b7e4575c98b87e09ac799c574d770aa5ee4df5e6c6cf36b6e277d92b8d2c2"
    }
    # Validate all bytes before creating any output. A bad EH input cannot
    # leave a partial MVP diagnostic directory that looks usable.
    for row in ($expected_assets | transpose name sha256) {
        let source = ($runtime | path join $row.name)
        if not ($source | path exists) { error make {msg: $"Missing installed asset: ($row.name)"} }
        let actual = (^sha256sum $source | split row ' ' | first)
        if $env.LAST_EXIT_CODE != 0 or $actual != $row.sha256 {
            error make {msg: $"Installed asset hash mismatch: ($row.name)"}
        }
    }
    let harness_html = ($env.FILE_PWD | path join "duckdb-worker-crypto-harness.html")
    let harness_js = ($env.FILE_PWD | path join "duckdb-worker-crypto-harness.mjs")
    if not ($harness_html | path exists) or not ($harness_js | path exists) {
        error make {msg: "Missing same-origin WebKit harness source"}
    }
    mkdir $output_dir
    for row in ($expected_assets | transpose name sha256) {
        cp ($runtime | path join $row.name) ($output_dir | path join $row.name)
        let copied = (^sha256sum ($output_dir | path join $row.name) | split row ' ' | first)
        if $env.LAST_EXIT_CODE != 0 or $copied != $row.sha256 {
            error make {msg: $"Staged asset copy mismatch: ($row.name)"}
        }
    }
    for variant in [mvp eh] {
        (open --raw $template | str replace --all '__VARIANT__' $variant) | save ($output_dir | path join $"duckdb-crypto-($variant).wrapper.js")
    }
    cp $harness_html ($output_dir | path join "index.html")
    cp $harness_js ($output_dir | path join "duckdb-worker-crypto-harness.mjs")
    let receipt = {
        schema: "tundra-duckdb-worker-crypto-input/v1"
        installed_source_head: "1ccac98a3cfe7a7b1e28b900761d81b94fa5f64f"
        installed_source_run: 40237343784
        profile: $profile
        installed_elf_sha256: $actual_elf
        installed_asset_sha256: $expected_assets
        wrapper_mvp_sha256: (^sha256sum ($output_dir | path join "duckdb-crypto-mvp.wrapper.js") | split row ' ' | first)
        wrapper_eh_sha256: (^sha256sum ($output_dir | path join "duckdb-crypto-eh.wrapper.js") | split row ' ' | first)
        harness_sha256: (^sha256sum ($output_dir | path join "duckdb-worker-crypto-harness.mjs") | split row ' ' | first)
        worker_bytes_modified: false
        admission: "NO"
    }
    $receipt | to json | save ($output_dir | path join "input-receipt.json")
}
