#!/usr/bin/env nu

# Regenerate every licence record that is pinned to the current Cargo.lock:
#   scripts/license_approvals.json                      (exact name/version/checksum approvals)
#   app/tauri/third-party-licenses/manifest.json        (+ one text directory per approved version)
#   app/tauri/third-party-licenses/selected-manifest.json (+ selected/**)
# Run from the repository root after any Cargo.lock change:
#   nu scripts/refresh_license_evidence.nu <work-dir> [--cargo-cache <registry/cache dir>]
# It never approves anything new: a shipped licence expression that no existing
# approval covers, a missing licence text, or a legal-text treatment whose
# hard-coded exact version no longer matches stops the run (exit 2) with the exact
# crate, version and expression. Texts are byte-for-byte from the crate archive
# (or the pinned upstream commit / official URL the previous manifest used).

const all_profiles = [
    x86_64-unknown-linux-gnu/default x86_64-unknown-linux-gnu/tauri-sounds
    aarch64-apple-darwin/default aarch64-apple-darwin/tauri-sounds
    x86_64-pc-windows-msvc/default x86_64-pc-windows-msvc/tauri-sounds
    wasm32-unknown-unknown/default
]
const linux_profiles = [x86_64-unknown-linux-gnu/default x86_64-unknown-linux-gnu/tauri-sounds]
const tpl = 'app/tauri/third-party-licenses'
const licence_like = '(?i)^(licen[cs]e|copying|copyright|unlicense|notice)'

def fail [message: string] {
    error make {msg: $message}
}

def sha256 [file: string] {
    ^sha256sum $file | split row ' ' | first
}

def dir_for [name: string, version: string] {
    $'($name)-($version | str replace --all "+" "_")'
}

def fetch [url: string, out: string] {
    mkdir ($out | path dirname)
    ^curl --fail --location --silent --show-error --max-time 45 --retry 2 --output $out $url
    if $env.LAST_EXIT_CODE != 0 or not ($out | path exists) { fail $'Fetch failed: ($url)' }
}

def extract [archive: string, member: string, out: string] {
    mkdir ($out | path dirname)
    ^tar -xOzf $archive $member o> $out
    if $env.LAST_EXIT_CODE != 0 { fail $'Could not extract ($member) from ($archive)' }
}

def archive_for [name: string, version: string, checksum: string, cache: string, dl: string] {
    let hits = (glob ($cache | path join '*' $'($name)-($version).crate'))
    let path = if ($hits | length) == 1 { $hits | first } else {
        let p = ($dl | path join $'($name)-($version).crate')
        if not ($p | path exists) {
            fetch $'https://static.crates.io/crates/($name)/($name)-($version).crate' $p
        }
        $p
    }
    if (sha256 $path) != $checksum { fail $'Crate archive checksum mismatch: ($name)@($version)' }
    $path
}

def members_of [archive: string] {
    ^tar -tzf $archive | lines
}

# Top-level licence-like members of a crate archive (LICENSE*, COPYING*, COPYRIGHT*, UNLICENSE*, NOTICE*).
def licence_members [name: string, version: string, members: list<string>] {
    let prefix = $'($name)-($version)/'
    let plen = ($prefix | str length)
    $members
    | where {|m| ($m | str starts-with $prefix) and (not ($m | str ends-with '/'))}
    | where {|m| let rel = ($m | str substring $plen..); (not ($rel | str contains '/')) and ($rel =~ $licence_like)}
    | sort
}

def repin [template: record, lock_pkg: record, profiles: list<string>] {
    let suffix = $'-($template.version)'
    if not ($template.id | str ends-with $suffix) { fail $'Approval id does not end with its version: ($template.id)' }
    let keep = (($template.id | str length) - ($suffix | str length))
    let base = ($template.id | split chars | first $keep | str join)
    $template
    | upsert version $lock_pkg.version
    | upsert lock_checksum $lock_pkg.checksum
    | upsert id $'($base)-($lock_pkg.version)'
    | upsert package_id $'($template.source)#($template.name)@($lock_pkg.version)'
    | upsert profiles $profiles
}

def run_inventories [meta_dir: string, out_dir: string] {
    mkdir $out_dir
    mut results = []
    for combo in $all_profiles {
        let parts = ($combo | split row '/')
        let target = ($parts | get 0)
        let profile = ($parts | get 1)
        let out = ($out_dir | path join $'inv-($target)-($profile).json')
        if ($out | path exists) { rm --force $out }
        let r = (^nu scripts/license_inventory.nu ($meta_dir | path join $'meta-($target)-($profile).json') $target $profile $out | complete)
        if not ($out | path exists) { fail $'Inventory produced no output for ($combo): ($r.stdout) ($r.stderr)' }
        $results = ($results | append {profile: $combo, exit: $r.exit_code, inv: (open --raw $out | from json), log: $r.stdout})
    }
    $results
}

def main [work_dir: string, --cargo-cache: string, --pending: string = ""] {
    # --pending name@version,...: reviews left unapproved and tolerated here (the strict inventory keeps failing on them)
    let pending = ($pending | split row "," | where $it != "")
    let cargo_home = (($env | get -o CARGO_HOME) | default ($env.HOME | path join ".cargo"))
    let cache = if $cargo_cache == null { $cargo_home | path join 'registry' 'cache' } else { $cargo_cache }
    let work = ($work_dir | path expand)
    let meta_dir = ($work | path join 'meta')
    let inv_dir = ($work | path join 'inventory')
    let dl = ($work | path join 'crates')
    mkdir $meta_dir $dl

    # 0. locked cargo metadata for the seven inventory profiles
    for combo in $all_profiles {
        let parts = ($combo | split row '/')
        let target = ($parts | get 0)
        let profile = ($parts | get 1)
        let out = ($meta_dir | path join $'meta-($target)-($profile).json')
        if $profile == 'tauri-sounds' {
            ^cargo metadata --locked --format-version 1 --filter-platform $target --features at-tauri/sounds o> $out
        } else {
            ^cargo metadata --locked --format-version 1 --filter-platform $target o> $out
        }
        if $env.LAST_EXIT_CODE != 0 { fail $'cargo metadata failed for ($combo)' }
    }
    ^cargo fetch --locked --target x86_64-unknown-linux-gnu
    if $env.LAST_EXIT_CODE != 0 { fail 'cargo fetch failed' }

    let lock = (open --raw Cargo.lock | from toml | get package)
    let old_doc = (open --raw scripts/license_approvals.json | from json)
    let old_approvals = $old_doc.approvals

    # 1a. interim approvals: every locked version of every approved crate, same licence choice
    let approved_keys = ($old_approvals | each {|a| {name: $a.name, source: $a.source}} | uniq)
    mut candidates = []
    for key in $approved_keys {
        let pool = ($old_approvals | where {|a| $a.name == $key.name and $a.source == $key.source})
        let versions = ($lock | where {|p| $p.name == $key.name and (($p | get -o source) == $key.source)})
        for p in $versions {
            let same = ($pool | where {|a| $a.version == $p.version})
            let template = if ($same | is-empty) { $pool | first } else { $same | first }
            $candidates = ($candidates | append (repin $template $p $all_profiles))
        }
    }
    ($old_doc | upsert approvals ($candidates | sort-by name version)) | to json --indent 2 | $"($in)\n" | save --force scripts/license_approvals.json

    let interim = (run_inventories $meta_dir $inv_dir)
    mut reviews = []
    for r in $interim {
        for row in ($r.inv.packages | where product_policy == 'review-required') {
            if $"($row.name)@($row.version)" in $pending { continue }
            $reviews = ($reviews | append {profile: $r.profile, name: $row.name, version: $row.version, license: $row.license})
        }
    }
    if not ($reviews | is-empty) {
        $old_doc | to json --indent 2 | $"($in)\n" | save --force scripts/license_approvals.json
        print 'OWNER DECISION REQUIRED (not approved by this script):'
        for item in ($reviews | uniq) { print $'  ($item.name)@($item.version) license "($item.license)" in ($item.profile)' }
        exit 2
    }

    # 1b. keep only approvals the inventory actually used; profiles come from the inventory
    mut used_profiles = {}
    for r in $interim {
        for id in $r.inv.used_approval_ids {
            let prior = ($used_profiles | get -o $id | default [])
            $used_profiles = ($used_profiles | upsert $id ($prior | append $r.profile))
        }
    }
    let used = $used_profiles
    let approvals = ($candidates
        | where {|a| ($used | get -o $a.id) != null}
        | each {|a| $a | upsert profiles ($all_profiles | where {|p| $p in ($used | get $a.id)} | sort)}
        | sort-by name version)
    ($old_doc | upsert approvals $approvals) | to json --indent 2 | $"($in)\n" | save --force scripts/license_approvals.json
    let final = (run_inventories $meta_dir $inv_dir)
    for r in $final {
        let open_reviews = ($r.inv.packages | where product_policy == "review-required" | where {|p| $"($p.name)@($p.version)" not-in $pending})
        if not ($open_reviews | is-empty) or not ($r.inv.unused_approval_ids | is-empty) {
            fail $'Final inventory not clean for ($r.profile): ($r.log)'
        }
    }
    for a in $approvals {
        let old = ($old_approvals | where {|o| $o.name == $a.name and $o.version == $a.version})
        if ($old | is-empty) { print $'APPROVAL ADDED ($a.id) [($a.approved_choice | str join ",")]' }
    }
    for o in $old_approvals {
        if not ($approvals | any {|a| $a.id == $o.id}) { print $'APPROVAL RE-PINNED/DROPPED ($o.id)' }
    }

    # 2. exception texts: manifest.json + one directory per approved version
    let old_manifest = (open --raw $'($tpl)/manifest.json' | from json)
    mut problems = []
    mut entries = []
    for a in $approvals {
        let archive = (archive_for $a.name $a.version $a.lock_checksum $cache $dl)
        let members = (members_of $archive)
        let olds = ($old_manifest.entries | where {|e| $e.name == $a.name})
        let exact = ($olds | where {|e| $e.version == $a.version and $e.crate_sha256 == $a.lock_checksum})
        if not ($exact | is-empty) {
            $entries = ($entries | append ($exact | first))
            continue
        }
        let prefix = $'($a.name)-($a.version)/'
        let plen = ($prefix | str length)
        mut member = ''
        mut url = ''
        mut base = 'LICENSE'
        if not ($olds | is-empty) {
            let old = ($olds | sort-by version | last)
            $base = ($old.file | path basename)
            if ($old.source | str starts-with 'crate:') {
                let rel = ($old.source | str replace $'crate:($old.name)-($old.version)/' '')
                if $'($prefix)($rel)' in $members { $member = $'($prefix)($rel)'; $base = $rel }
            } else {
                $url = $old.source
            }
        }
        if $member == '' {
            let found = (licence_members $a.name $a.version $members)
            if ($found | length) == 1 {
                $member = ($found | first)
                $base = ($member | str substring $plen..)
            } else if ($found | is-empty) and $url != '' {
                # no licence file in the crate archive: official text, as the previous manifest did
            } else {
                $problems = ($problems | append $'Exception text for ($a.name)@($a.version): ($found | length) licence-like members and no previous treatment applies')
                continue
            }
        }
        let file = $'($tpl)/(dir_for $a.name $a.version)/($base)'
        let source = if $member != '' { extract $archive $member $file; $'crate:($member)' } else { fetch $url $file; $url }
        $entries = ($entries | append {
            approval_id: $a.id
            name: $a.name
            version: $a.version
            license_id: ($a.approved_choice | first)
            crate_sha256: $a.lock_checksum
            file: $file
            text_sha256: (sha256 $file)
            source: $source
        })
    }
    if not ($problems | is-empty) { for p in $problems { print $'OWNER DECISION REQUIRED: ($p)' }; exit 2 }
    let entries_final = $entries
    ($old_manifest | upsert entries ($entries | sort-by name version)) | to json --indent 2 | $"($in)\n" | save --force $'($tpl)/manifest.json'
    let keep_dirs = ($entries | each {|e| $e.file | path dirname | path basename} | uniq)
    # only directories the previous manifest owned; other files here (e.g. ui/ notices) are not ours
    let old_dirs = ($old_manifest.entries | each {|e| $e.file | path dirname | path basename} | uniq)
    for b in ($old_dirs | where {|b| $b not-in $keep_dirs}) {
        let d = ($tpl | path join $b)
        if ($d | path exists) { rm --recursive --force $d; print $"REMOVED ($d)" }
    }

    # 3. selected-source bill: every product package in both Linux profiles
    let old_sel = (open --raw $'($tpl)/selected-manifest.json' | from json)
    let flat = ($linux_profiles | each {|prof|
        let inv = ($final | where profile == $prof | first | get inv)
        $inv.packages | where {|p| $p.role == 'product' and (not $p.first_party)} | each {|p|
            $p | insert package_id $'($p.source)#($p.name)@($p.version)' | insert profile $prof
        }
    } | flatten)
    let grouped = ($flat | group-by package_id | transpose id rows | sort-by id)
    mut sel_entries = []
    mut stats = {reused: 0, remapped: 0, fresh: 0, exception: 0}
    for g in $grouped {
        let row = ($g.rows | first)
        let ids = ($row.selected_license_ids | sort)
        for other in $g.rows {
            if ($other.selected_license_ids | sort) != $ids { $problems = ($problems | append $'Selected licences differ between profiles: ($g.id)') }
        }
        let locked = ($lock | where {|p| $p.name == $row.name and $p.version == $row.version and ($p | get -o source) == $row.source})
        if ($locked | length) != 1 { fail $'Cargo.lock tuple absent or duplicate: ($g.id)' }
        let checksum = ($locked | first).checksum
        let profiles = ($linux_profiles | where {|p| $p in ($g.rows | get profile)})
        let olds = ($old_sel.entries | where {|e| $e.name == $row.name})
        let exact = ($olds | where {|e| $e.version == $row.version and $e.lock_checksum == $checksum})
        mut texts = []
        let approval_ids = ($g.rows | get approval_id | where {|i| $i != null} | uniq)
        if not ($approval_ids | is-empty) {
            let m = ($entries_final | where {|e| $e.approval_id == ($approval_ids | first)} | first)
            $texts = [{file: $m.file, text_sha256: $m.text_sha256, source_kind: 'verified-exception'}]
            $stats = ($stats | upsert exception ($stats.exception + 1))
        } else if not ($exact | is-empty) {
            $texts = ($exact | first).texts
            if (($exact | first).selected_license_ids | sort) != $ids { $problems = ($problems | append $'Selected licences changed for unchanged package: ($g.id)') }
            $stats = ($stats | upsert reused ($stats.reused + 1))
        } else {
            let archive = (archive_for $row.name $row.version $checksum $cache $dl)
            let members = (members_of $archive)
            let prefix = $'($row.name)-($row.version)/'
            let plen = ($prefix | str length)
            let sel_dir = $'($tpl)/selected/(dir_for $row.name $row.version)'
            mut remapped = false
            if not ($olds | is-empty) {
                let old = ($olds | sort-by version | last)
                let kinds = ($old.texts | get source_kind | uniq)
                if ($kinds | any {|k| $k in ['vcs-tag' 'standard-apache' 'verified-exception']}) {
                    $problems = ($problems | append $'Legal-text treatment ($kinds | str join ",") of ($old.package_id) does not carry to ($g.id): its verifier pins exact versions')
                    continue
                }
                mut new_texts = []
                mut ok = true
                for t in $old.texts {
                    if $t.source_kind == 'crate' {
                        let rel = ($t.crate_member | str replace $'($old.name)-($old.version)/' '')
                        let m = $'($prefix)($rel)'
                        if $m not-in $members { $ok = false; break }
                        let file = $'($sel_dir)/($rel)'
                        extract $archive $m $file
                        $new_texts = ($new_texts | append {file: $file, text_sha256: (sha256 $file), source_kind: 'crate', crate_member: $m})
                    } else if $t.source_kind == 'vcs-root' {
                        let info = (^tar -xOzf $archive $'($prefix).cargo_vcs_info.json' | from json)
                        let commit = $info.git.sha1
                        let rel = ($t.file | str replace $'($tpl)/selected/(dir_for $old.name $old.version)/' '')
                        let file = $'($sel_dir)/($rel)'
                        if $commit == $t.vcs_commit and ($t.file | path exists) {
                            mkdir ($file | path dirname)
                            cp $t.file $file
                        } else {
                            fetch $'https://raw.githubusercontent.com/($t.vcs_repo)/($commit)/($t.vcs_license_path)' $file
                        }
                        $new_texts = ($new_texts | append {file: $file, text_sha256: (sha256 $file), source_kind: 'vcs-root', vcs_repo: $t.vcs_repo, vcs_commit: $commit, vcs_license_path: $t.vcs_license_path})
                    } else {
                        $ok = false; break
                    }
                }
                if $ok {
                    $texts = $new_texts
                    $remapped = true
                    $stats = ($stats | upsert remapped ($stats.remapped + 1))
                } else {
                    print $'FALLBACK ($g.id): previous members not all present, using licence-like members'
                }
            }
            if not $remapped {
                let found = (licence_members $row.name $row.version $members)
                if ($found | is-empty) {
                    $problems = ($problems | append $'No licence text in crate archive: ($g.id) [($row.license)]')
                    continue
                }
                for m in $found {
                    let rel = ($m | str substring $plen..)
                    let file = $'($sel_dir)/($rel)'
                    extract $archive $m $file
                    $texts = ($texts | append {file: $file, text_sha256: (sha256 $file), source_kind: 'crate', crate_member: $m})
                }
                $stats = ($stats | upsert fresh ($stats.fresh + 1))
            }
        }
        $sel_entries = ($sel_entries | append {
            package_id: $g.id
            name: $row.name
            version: $row.version
            source: $row.source
            lock_checksum: $checksum
            profiles: $profiles
            selected_license_ids: $row.selected_license_ids
            texts: $texts
        })
    }
    if not ($problems | is-empty) { for p in ($problems | uniq) { print $'OWNER DECISION REQUIRED: ($p)' }; exit 2 }
    ($old_sel | upsert entries $sel_entries) | to json --indent 2 | $"($in)\n" | save --force $'($tpl)/selected-manifest.json'

    # drop selected/** files no entry refers to
    let referenced = ($sel_entries | get texts | flatten | get file | uniq)
    mut removed = 0
    for f in (glob $'($tpl)/selected/**/*' | where {|p| ($p | path type) == file}) {
        let rel = ($f | path relative-to (pwd))
        if $rel not-in $referenced { rm --force $f; $removed += 1 }
    }
    for d in (glob $'($tpl)/selected/**/*' | where {|p| ($p | path type) == dir} | sort | reverse) {
        if (ls $d | is-empty) { rm --recursive --force $d }
    }
    print $'selected-manifest: ($sel_entries | length) entries, ($stats | to nuon), ($removed) obsolete files removed'

    # 4. run the repository verifiers for both Linux profiles
    let lin = ($final | where {|r| $r.profile in $linux_profiles})
    for pair in [[0 1] [1 0]] {
        let cur = ($lin | get ($pair | first))
        let peer = ($lin | get ($pair | last))
        let cp = ($cur.profile | split row '/')
        let pp = ($peer.profile | split row '/')
        let bill = ($work | path join $'bill-($cp | get 1).json')
        ^nu scripts/selected_notice_bill.nu ($inv_dir | path join $'inv-($cp | get 0)-($cp | get 1).json') ($inv_dir | path join $'inv-($pp | get 0)-($pp | get 1).json') ($meta_dir | path join $'meta-($cp | get 0)-($cp | get 1).json') ($meta_dir | path join $'meta-($pp | get 0)-($pp | get 1).json') Cargo.lock $'($tpl)/selected-manifest.json' $'($tpl)/manifest.json' $cache $bill
        if $env.LAST_EXIT_CODE != 0 { fail $'selected_notice_bill.nu failed for ($cur.profile)' }
    }
    print 'refresh complete'
}
