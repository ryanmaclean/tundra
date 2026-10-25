#!/usr/bin/env nu

# Candidate for PR105. Run on hosted Linux before cargo tauri build.
# A missing package, crate archive, legal text, or installed-source binding is fatal.

def fail [message: string] {
    error make {msg: $message}
}

def sha256 [file: string] {
    if not ($file | path exists) { fail $'Missing file: ($file)' }
    let value = (^sha256sum $file | split row ' ' | first)
    if not ($value =~ '^[0-9a-f]{64}$') { fail $'Invalid sha256sum result for ($file)' }
    $value
}

def new_temp [] {
    let file = (^mktemp | str trim)
    if $env.LAST_EXIT_CODE != 0 or ($file | is-empty) { fail 'Could not create source-custody temporary file' }
    $file
}

def fetch_temp [url: string] {
    let file = (new_temp)
    ^curl --fail --location --silent --show-error --max-time 30 --output $file $url
    let fetched = ($env.LAST_EXIT_CODE == 0)
    ^test -s $file
    let nonempty = ($env.LAST_EXIT_CODE == 0)
    if not $fetched or not $nonempty {
        ^rm --force -- $file
        fail $'Pinned source fetch failed or was empty: ($url)'
    }
    $file
}

def member_matches_file [archive: string, member: string, expected: string] {
    let file = (new_temp)
    ^tar -xOzf $archive $member o> $file
    let extracted = ($env.LAST_EXIT_CODE == 0)
    mut equal = false
    if $extracted {
        ^cmp -- $file $expected
        $equal = ($env.LAST_EXIT_CODE == 0)
    }
    ^rm --force -- $file
    $extracted and $equal
}

def fetch_matches_file [url: string, expected: string] {
    ^test -s $expected
    if $env.LAST_EXIT_CODE != 0 { fail $'Expected legal text is empty: ($expected)' }
    let file = (fetch_temp $url)
    ^cmp -- $file $expected
    let equal = ($env.LAST_EXIT_CODE == 0)
    ^rm --force -- $file
    $equal
}

def package_id [row: record] {
    $'($row.source)#($row.name)@($row.version)'
}

def safe_resource [file: string] {
    ($file | str starts-with 'app/tauri/third-party-licenses/selected/') and (not ($file | str contains '/../')) and ($file =~ '^app/tauri/third-party-licenses/selected/[A-Za-z0-9._/-]+$')
}

def main [
    inventory_path: string
    peer_inventory_path: string
    metadata_path: string
    peer_metadata_path: string
    lock_path: string
    selected_manifest_path: string
    exception_manifest_path: string
    crate_cache_root: string
    output_path: string
] {
    let inventory = (open --raw $inventory_path | from json)
    let peer_inventory = (open --raw $peer_inventory_path | from json)
    let lock = (open --raw $lock_path | from toml)
    let selected_manifest = (open --raw $selected_manifest_path | from json)
    let exception_manifest = (open --raw $exception_manifest_path | from json)

    if $selected_manifest.schema_version != 1 { fail 'Unsupported selected notice manifest schema' }
    if $inventory.cargo_lock_sha256 != (sha256 $lock_path) { fail 'Inventory Cargo.lock digest mismatch' }
    if $inventory.cargo_metadata_sha256 != (sha256 $metadata_path) { fail 'Inventory cargo metadata digest mismatch' }
    if $peer_inventory.cargo_lock_sha256 != (sha256 $lock_path) { fail 'Peer inventory Cargo.lock digest mismatch' }
    if $peer_inventory.cargo_metadata_sha256 != (sha256 $peer_metadata_path) { fail 'Peer inventory cargo metadata digest mismatch' }
    if ($inventory.review_required_count != 0) or (not ($inventory.unused_approval_ids | is-empty)) {
        fail 'Product license inventory has reviews or stale approvals'
    }
    if ($peer_inventory.review_required_count != 0) or (not ($peer_inventory.unused_approval_ids | is-empty)) {
        fail 'Peer product license inventory has reviews or stale approvals'
    }

    let current_profile = $'($inventory.target)/($inventory.feature_profile)'
    let peer_profile = $'($peer_inventory.target)/($peer_inventory.feature_profile)'
    let allowed_profiles = ['x86_64-unknown-linux-gnu/default' 'x86_64-unknown-linux-gnu/tauri-sounds']
    if (($current_profile not-in $allowed_profiles) or ($peer_profile not-in $allowed_profiles) or ($current_profile == $peer_profile)) {
        fail 'Expected the two distinct Linux default/sounds profiles'
    }

    let selected = ($inventory.packages | where {|p| $p.role == 'product' and (not $p.first_party)})
    let peer_selected = ($peer_inventory.packages | where {|p| $p.role == 'product' and (not $p.first_party)})
    let selected_ids = ($selected | each {|p| package_id $p})
    let peer_selected_ids = ($peer_selected | each {|p| package_id $p})
    let manifest_ids = ($selected_manifest.entries | each {|e| $e.package_id})
    if ($selected_ids | uniq | length) != ($selected_ids | length) { fail 'Duplicate selected package ID' }
    if ($peer_selected_ids | uniq | length) != ($peer_selected_ids | length) { fail 'Duplicate peer selected package ID' }
    if ($manifest_ids | uniq | length) != ($manifest_ids | length) { fail 'Duplicate bill package ID' }
    for entry in $selected_manifest.entries {
        if ($entry.profiles | is-empty) or (($entry.profiles | uniq | length) != ($entry.profiles | length)) {
            fail $'Missing or duplicate bill profile membership: ($entry.package_id)'
        }
        for profile in $entry.profiles {
            if $profile not-in $allowed_profiles { fail $'Unknown bill profile: ($entry.package_id)' }
            let expected_ids = if $profile == $current_profile { $selected_ids } else { $peer_selected_ids }
            if $entry.package_id not-in $expected_ids {
                fail $'Bill row is not selected in its declared profile: ($entry.package_id) / ($profile)'
            }
        }
    }
    let current_entries = ($selected_manifest.entries | where {|e| $current_profile in $e.profiles})
    let current_manifest_ids = ($current_entries | each {|e| $e.package_id})
    let missing = ($selected_ids | where {|id| $id not-in $current_manifest_ids})
    let stale = ($current_manifest_ids | where {|id| $id not-in $selected_ids})
    if not ($missing | is-empty) { fail $'Missing selected-source bill rows: ($missing | str join ", ")' }
    if not ($stale | is-empty) { fail $'Stale or out-of-profile bill rows: ($stale | str join ", ")' }

    mut verified_rows = []
    mut all_files = []
    for row in $selected {
        let id = (package_id $row)
        let entry = ($current_entries | where {|e| $e.package_id == $id} | first)
        if $entry.name != $row.name or $entry.version != $row.version or $entry.source != $row.source {
            fail $'Identity mismatch: ($id)'
        }
        if ($entry.selected_license_ids | sort) != ($row.selected_license_ids | sort) {
            fail $'Selected license mismatch: ($id)'
        }
        if $row.source != 'registry+https://github.com/rust-lang/crates.io-index' {
            fail $'No source custody implementation for ($id)'
        }
        let locked = ($lock.package | where {|p| $p.name == $row.name and $p.version == $row.version and $p.source == $row.source})
        if ($locked | length) != 1 { fail $'Cargo.lock tuple absent or duplicate: ($id)' }
        if $entry.lock_checksum != (($locked | first).checksum) { fail $'Cargo.lock checksum mismatch: ($id)' }
        if not ($entry.lock_checksum =~ '^[0-9a-f]{64}$') { fail $'Invalid archive checksum: ($id)' }
        if ($entry.texts | is-empty) { fail $'No legal text for selected package: ($id)' }

        let archive_name = $'($row.name)-($row.version).crate'
        let archives = (glob ($crate_cache_root | path join '*' $archive_name))
        if ($archives | length) != 1 { fail $'Expected one cached crate archive for ($id)' }
        let archive = ($archives | first)
        if (sha256 $archive) != $entry.lock_checksum { fail $'Crate archive checksum mismatch: ($id)' }

        mut text_rows = []
        for item in $entry.texts {
            if ($item.source_kind in ['crate' 'vcs-root' 'vcs-tag' 'standard-apache']) and (not (safe_resource $item.file)) {
                fail $'Unsafe selected legal-text resource path: ($id)'
            }
            if $item.source_kind == 'verified-exception' and (not ($item.file | str starts-with 'app/tauri/third-party-licenses/') or ($item.file | str contains '/../')) { fail $'Unsafe exception legal-text resource path: ($id)' }
            if not ($item.text_sha256 =~ '^[0-9a-f]{64}$') { fail $'Invalid legal-text digest: ($id)' }
            if (sha256 $item.file) != $item.text_sha256 { fail $'Legal-text digest mismatch: ($id)' }
            if $item.source_kind == 'crate' {
                let prefix = $'($row.name)-($row.version)/'
                if not ($item.crate_member | str starts-with $prefix) or ($item.crate_member | str contains '/../') {
                    fail $'Unsafe crate member: ($id)'
                }
                if not (member_matches_file $archive $item.crate_member $item.file) {
                    fail $'Crate member extraction or source/text byte mismatch: ($id)'
                }
            } else if $item.source_kind == 'verified-exception' {
                let exact = ($exception_manifest.entries | where {|e| $e.name == $row.name and $e.version == $row.version and $e.crate_sha256 == $entry.lock_checksum and $e.text_sha256 == $item.text_sha256 and $e.file == $item.file })
                if ($exact | length) != 1 { fail $'Existing exception custody mismatch: ($id)' }
                # The separate exact-notice-custody job verifies its official/crate bytes.
            } else if $item.source_kind in ['vcs-root' 'vcs-tag'] {
                if not ($item.vcs_repo =~ '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') or not ($item.vcs_commit =~ '^[0-9a-f]{40}$') or not ($item.vcs_license_path =~ '^[A-Za-z0-9._/-]+$') or ($item.vcs_license_path | str contains '/../') {
                    fail $'Unsafe upstream legal-text identity: ($id)'
                }
                if $item.source_kind == 'vcs-root' {
                    let member = $'($row.name)-($row.version)/.cargo_vcs_info.json'
                    let vcs_file = (new_temp)
                    ^tar -xOzf $archive $member o> $vcs_file
                    let extracted = ($env.LAST_EXIT_CODE == 0)
                    ^test -s $vcs_file
                    let nonempty = ($env.LAST_EXIT_CODE == 0)
                    if not $extracted or not $nonempty {
                        ^rm --force -- $vcs_file
                        fail $'Missing or failed crate VCS identity: ($id)'
                    }
                    let vcs = (open --raw $vcs_file | from json)
                    ^rm --force -- $vcs_file
                    if $vcs.git.sha1 != $item.vcs_commit { fail $'Crate/upstream commit mismatch: ($id)' }
                } else {
                    if $row.name != 'serial' or $row.version != '0.4.0' or $item.vcs_repo != 'dcuddeback/serial-rs' or $item.tag != 'v0.4.0' {
                        fail $'Unreviewed VCS tag treatment: ($id)'
                    }
                    let tag_lines = (^git ls-remote 'https://github.com/dcuddeback/serial-rs.git' 'refs/tags/v0.4.0^{}' | lines)
                    if $env.LAST_EXIT_CODE != 0 or ($tag_lines | length) != 1 or (($tag_lines | first | split row (char tab) | first) != $item.vcs_commit) {
                        fail $'Serial tag no longer resolves to pinned commit: ($id)'
                    }
                    let expected_members = [
                        'serial-0.4.0/Cargo.toml'
                        'serial-0.4.0/README.md'
                        'serial-0.4.0/examples/open_port.rs'
                        'serial-0.4.0/examples/probe_pins.rs'
                        'serial-0.4.0/examples/read_write.rs'
                        'serial-0.4.0/src/lib.rs'
                    ]
                    if ($item.crate_members | sort) != ($expected_members | sort) {
                        fail $'Serial crate member custody list mismatch: ($id)'
                    }
                    let actual_members = (^tar -tzf $archive | lines | sort)
                    if $env.LAST_EXIT_CODE != 0 or $actual_members != ($expected_members | sort) {
                        fail $'Serial crate archive content differs from reviewed member set: ($id)'
                    }
                    for member in $expected_members {
                        let relative = ($member | str replace 'serial-0.4.0/' '')
                        let source_url = $'https://raw.githubusercontent.com/($item.vcs_repo)/($item.vcs_commit)/serial/($relative)'
                        let upstream = (fetch_temp $source_url)
                        let equal = (member_matches_file $archive $member $upstream)
                        ^rm --force -- $upstream
                        if not $equal { fail $'Serial archive/source member byte mismatch: ($id) / ($relative)' }
                    }
                }
                let text_url = $'https://raw.githubusercontent.com/($item.vcs_repo)/($item.vcs_commit)/($item.vcs_license_path)'
                if not (fetch_matches_file $text_url $item.file) { fail $'Pinned upstream legal-text byte mismatch: ($id)' }
            } else if $item.source_kind == 'standard-apache' {
                let exact_eventsource = ($row.name == 'eventsource-stream' and $row.version == '0.2.3' and $row.source == 'registry+https://github.com/rust-lang/crates.io-index' and $entry.lock_checksum == '74fef4569247a5f429d9156b9d0a2599914385dd189c539334c625d8099d90ab' and $row.license == 'MIT OR Apache-2.0')
                let exact_http_serde = ($row.name == 'http-serde' and $row.version == '2.1.1' and $row.source == 'registry+https://github.com/rust-lang/crates.io-index' and $entry.lock_checksum == '0f056c8559e3757392c8d091e796416e4649d8e49e88b8d76df6c002f05027fd' and $row.license == 'Apache-2.0 OR MIT')
                if $item.official_url != 'https://www.apache.org/licenses/LICENSE-2.0.txt' or $item.file != 'app/tauri/third-party-licenses/selected/standard/Apache-2.0.txt' or $item.text_sha256 != 'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30' or $entry.selected_license_ids != ['Apache-2.0'] or $item.declared_license != $row.license or $item.crate_manifest_member != $'($row.name)-($row.version)/Cargo.toml' or not ($exact_eventsource or $exact_http_serde) {
                    fail $'Unreviewed standard Apache-2.0 treatment: ($id)'
                }
                let manifest_file = (new_temp)
                ^tar -xOzf $archive $item.crate_manifest_member o> $manifest_file
                let extracted = ($env.LAST_EXIT_CODE == 0)
                ^test -s $manifest_file
                let nonempty = ($env.LAST_EXIT_CODE == 0)
                if not $extracted or not $nonempty {
                    ^rm --force -- $manifest_file
                    fail $'Exact crate manifest unavailable for standard license: ($id)'
                }
                let crate_manifest = (open --raw $manifest_file | from toml)
                ^rm --force -- $manifest_file
                if $crate_manifest.package.name != $row.name or $crate_manifest.package.version != $row.version or $crate_manifest.package.license != $item.declared_license or $crate_manifest.package.authors != $item.declared_authors {
                    fail $'Crate license declaration mismatch for standard text: ($id)'
                }
                if $row.name == 'eventsource-stream' {
                    if $item.crate_readme_member != 'eventsource-stream-0.2.3/README.md' or $item.readme_license_statement != 'License: MIT OR Apache-2.0' {
                        fail $'Wrong eventsource README custody selector: ($id)'
                    }
                    let readme_file = (new_temp)
                    ^tar -xOzf $archive $item.crate_readme_member o> $readme_file
                    let readme_extracted = ($env.LAST_EXIT_CODE == 0)
                    if not $readme_extracted {
                        ^rm --force -- $readme_file
                        fail $'Eventsource README extraction failed: ($id)'
                    }
                    let readme = (open --raw $readme_file)
                    ^rm --force -- $readme_file
                    if not ($readme | str contains $item.readme_license_statement) {
                        fail $'Eventsource README license declaration mismatch: ($id)'
                    }
                }
                if not (fetch_matches_file $item.official_url $item.file) {
                    fail $'Official Apache-2.0 terms byte mismatch: ($id)'
                }
            } else {
                fail $'Unsupported legal-text source kind: ($id)'
            }
            $all_files = ($all_files | append {file: $item.file, text_sha256: $item.text_sha256})
            $text_rows = ($text_rows | append $item)
        }
        $verified_rows = ($verified_rows | append {
            package_id: $id
            name: $row.name
            version: $row.version
            source: $row.source
            lock_checksum: $entry.lock_checksum
            selected_license_ids: $row.selected_license_ids
            notice_ids: $row.notice_ids
            texts: $text_rows
        })
    }

    for item in ($all_files | group-by file | values) {
        if ($item | get text_sha256 | uniq | length) != 1 { fail 'One resource path has conflicting text hashes' }
    }
    let bill = {
        schema_version: 1
        target: $inventory.target
        feature_profile: $inventory.feature_profile
        cargo_lock_sha256: $inventory.cargo_lock_sha256
        cargo_metadata_sha256: $inventory.cargo_metadata_sha256
        selected_manifest_sha256: (sha256 $selected_manifest_path)
        selected_third_party_count: ($verified_rows | length)
        packages: ($verified_rows | sort-by package_id)
    }
    $bill | to json --indent 2 | save --force $output_path
    print $'Verified selected-source bill for ($bill.selected_third_party_count) packages: ($bill.target)/($bill.feature_profile)'
}
