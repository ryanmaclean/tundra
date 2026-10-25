#!/usr/bin/env nu

# Verify the exact selected DuckDB legal bill and its installed readback.
# The workflow independently pins this bill's SHA-256 before invoking this script.
def digest [file: string] {
    let out = (^sha256sum $file | str trim | split row ' ' | first)
    if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Hash failed: ($file)"} }
    $out
}

def main [scratch_dir: string, installed_root?: string] {
    let bill_path = "app/tauri/third-party-licenses/ui/duckdb-runtime-selected-bill.json"
    let bill = (open $bill_path)
    if $bill.schema != "tundra-duckdb-selected-notice-bill/v2" or $bill.admission != "NO" {
        error make {msg: "Unexpected DuckDB bill schema/admission"}
    }
    if $bill.profiles != [default sounds] or ($bill.assets | length) != 5 or ($bill.attributions | length) != 38 or ($bill.texts | length) != 36 or ($bill.unresolved | length) < 2 {
        error make {msg: "DuckDB selected-bill cardinality/profile mismatch"}
    }
    let prefix = "third-party-licenses/ui/duckdb-runtime/"
    let names = ($bill.texts | get file | sort)
    if ($names | uniq | length) != 36 { error make {msg: "Duplicate DuckDB legal text path"} }
    let actual_files = (glob "app/tauri/third-party-licenses/ui/duckdb-runtime/*" | sort)
    if ($actual_files | length) != 36 { error make {msg: "DuckDB legal directory has missing/extra files"} }
    mkdir $scratch_dir
    for text in $bill.texts {
        if not ($text.file | str starts-with $prefix) or ($text.file | str contains "..") or ($text.file | str contains "\\") {
            error make {msg: $"Unsafe DuckDB legal path: ($text.file)"}
        }
        let source = $"app/tauri/($text.file)"
        if ($source | path type) != "file" or (digest $source) != $text.sha256 {
            error make {msg: $"DuckDB source legal text mismatch: ($source)"}
        }
        if ($installed_root | is-not-empty) {
            let matches = (glob $"($installed_root)/**/($text.file)")
            if ($matches | length) != 1 { error make {msg: $"DuckDB installed legal text count mismatch: ($text.file)"} }
            if ($matches.0 | path type) != "file" or (digest $matches.0) != $text.sha256 {
                error make {msg: $"DuckDB installed legal text hash mismatch: ($text.file)"}
            }
            ^cmp $source $matches.0
            if $env.LAST_EXIT_CODE != 0 { error make {msg: $"DuckDB installed legal text differs: ($text.file)"} }
        }
    }
    let expected_files = ($bill.texts | each {|x| $"app/tauri/($x.file)"} | sort)
    if $actual_files != $expected_files { error make {msg: "DuckDB legal directory name set mismatch"} }
    let attribution_keys = ($bill.attributions | each {|x| $"($x.surface)|($x.component)|($x.source_url)|($x.file)"})
    if ($attribution_keys | uniq | length) != 38 { error make {msg: "Duplicate DuckDB attribution"} }
    for entry in ($bill.attributions | enumerate) {
        let attribution = $entry.item
        if ($attribution.license_or_role | str trim | is-empty) {
            error make {msg: $"Missing DuckDB license/notice role: ($attribution.component)"}
        }
        let matches = ($bill.texts | where file == $attribution.file)
        if ($matches | length) != 1 or $matches.0.sha256 != $attribution.source_sha256 {
            error make {msg: $"DuckDB attribution/text join mismatch: ($attribution.component)"}
        }
        let parts = ($attribution.source_url | split row "#")
        let archive = $"($scratch_dir)/source-($entry.index)"
        ^curl --fail --silent --show-error --location --max-time 90 ($parts | first) -o $archive
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"DuckDB source fetch failed: ($attribution.source_url)"} }
        if ($parts | length) == 2 {
            let extracted = $"($scratch_dir)/extracted-($entry.index)"
            mkdir $extracted
            ^tar -xzf $archive -C $extracted $parts.1
            if $env.LAST_EXIT_CODE != 0 { error make {msg: $"DuckDB source member missing: ($attribution.source_url)"} }
            ^cmp $"($extracted)/($parts.1)" $"app/tauri/($attribution.file)"
        } else {
            ^cmp $archive $"app/tauri/($attribution.file)"
        }
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"DuckDB upstream legal bytes differ: ($attribution.source_url)"} }
    }
    if ($installed_root | is-not-empty) {
        let installed = (glob $"($installed_root)/**/third-party-licenses/ui/duckdb-runtime/*")
        if ($installed | length) != 36 { error make {msg: "DuckDB installed legal directory has extra/missing files"} }
        let bills = (glob $"($installed_root)/**/third-party-licenses/ui/duckdb-runtime-selected-bill.json")
        if ($bills | length) != 1 { error make {msg: "Installed DuckDB bill count mismatch"} }
        ^cmp $bill_path $bills.0
        if $env.LAST_EXIT_CODE != 0 { error make {msg: "Installed DuckDB bill differs"} }
    }
}
