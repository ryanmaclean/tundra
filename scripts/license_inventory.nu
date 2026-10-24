#!/usr/bin/env nu

# CI-only, locked Cargo metadata classifier. Build dependencies are treated as
# product dependencies because generated code can be incorporated into output.
const base_allowed = [MIT BSD-2-Clause BSD-3-Clause Apache-2.0]
const approved_extra = [ISC CDLA-Permissive-2.0 Unicode-3.0]
const inventory_profiles = [
    x86_64-unknown-linux-gnu/default x86_64-unknown-linux-gnu/tauri-sounds
    aarch64-apple-darwin/default aarch64-apple-darwin/tauri-sounds
    x86_64-pc-windows-msvc/default x86_64-pc-windows-msvc/tauri-sounds
    wasm32-unknown-unknown/default
]
const reviewed_workspace_names = [
    at-core at-harness at-session at-agents at-daemon at-telemetry
    at-api-types at-cli at-bridge at-tui at-integrations at-intelligence
    at-tauri at-leptos-ui
]

def roots_for [target: string] {
    if $target == 'wasm32-unknown-unknown' {
        [at-leptos-ui]
    } else {
        [at-cli at-daemon at-tui at-tauri]
    }
}

def optional_dependency_active [package: record, node: record, dependency_key: string] {
    # A weak optional feature reference does not activate the dependency.
    if $dependency_key in $node.features { return true }
    for feature in $node.features {
        let entries = ($package.features | get -o $feature | default [])
        for entry in $entries {
            if $entry == $'dep:($dependency_key)' or $entry == $dependency_key or ($entry | str starts-with $'($dependency_key)/') {
                return true
            }
        }
    }
    false
}

def precedence [operator: string] {
    if $operator == 'AND' { 2 } else { 1 }
}

def license_evaluation [expression: string, approved_atoms: list<string>, notice_atoms: list<string>] {
    # A legacy slash is only accepted for a plain list of alternatives.
    # Mixing it with SPDX operators or grouping has ambiguous precedence;
    # fail closed rather than turning a mandatory term into an optional one.
    let legacy_slash = ($expression | str contains '/')
    let mixed_slash = (($expression | str contains '(') or ($expression | str contains ')') or ($expression =~ '\b(AND|OR|WITH)\b'))
    if $legacy_slash and $mixed_slash {
        return {valid: false, allowed: false, selected_ids: []}
    }
    # Tokenize unambiguous legacy alternatives as OR. An adjacent ID WITH
    # exception-ID becomes one deliberately unapproved atom. Reject malformed
    # or repeated WITH before evaluating any independent OR alternative.
    let raw_tokens = ($expression | str replace --all '/' ' OR ' | str replace --all '(' ' ( ' | str replace --all ')' ' ) ' | str replace --all --regex '\s+' ' ' | str trim | split row ' ' | where $it != '')
    if ($raw_tokens | is-empty) or ($expression | str contains '@') {
        return {valid: false, allowed: false, selected_ids: []}
    }
    if ('WITH' in $raw_tokens) and (($expression | str contains '(') or ($expression | str contains ')')) {
        return {valid: false, allowed: false, selected_ids: []}
    }
    mut tokens = []
    mut token_index = 0
    while $token_index < ($raw_tokens | length) {
        let token = ($raw_tokens | get $token_index)
        let next_index = ($token_index + 1)
        let next_token = if $next_index < ($raw_tokens | length) { $raw_tokens | get $next_index } else { '' }
        if $next_token == 'WITH' {
            let exception_index = ($token_index + 2)
            if ($token !~ '^[A-Za-z0-9][A-Za-z0-9.+-]*$') or ($token in [AND OR WITH]) or $exception_index >= ($raw_tokens | length) {
                return {valid: false, allowed: false, selected_ids: []}
            }
            let exception_id = ($raw_tokens | get $exception_index)
            if ($exception_id !~ '^[A-Za-z0-9][A-Za-z0-9.+-]*$') or ($exception_id in [AND OR WITH]) {
                return {valid: false, allowed: false, selected_ids: []}
            }
            $tokens = ($tokens | append $'@with:($token):($exception_id)')
            $token_index = ($token_index + 3)
        } else {
            if $token == 'WITH' { return {valid: false, allowed: false, selected_ids: []} }
            $tokens = ($tokens | append $token)
            $token_index = ($token_index + 1)
        }
    }
    mut output = []
    mut operators = []
    mut expect_id = true
    for token in $tokens {
        if $token == '(' {
            if not $expect_id { return {valid: false, allowed: false, selected_ids: []} }
            $operators = ($operators | append $token)
        } else if $token == ')' {
            if $expect_id { return {valid: false, allowed: false, selected_ids: []} }
            while not ($operators | is-empty) and (($operators | last) != '(') {
                $output = ($output | append ($operators | last))
                $operators = ($operators | drop 1)
            }
            if ($operators | is-empty) { return {valid: false, allowed: false, selected_ids: []} }
            $operators = ($operators | drop 1)
        } else if $token in [AND OR] {
            if $expect_id { return {valid: false, allowed: false, selected_ids: []} }
            while not ($operators | is-empty) and (($operators | last) != '(') and ((precedence ($operators | last)) >= (precedence $token)) {
                $output = ($output | append ($operators | last))
                $operators = ($operators | drop 1)
            }
            $operators = ($operators | append $token)
            $expect_id = true
        } else {
            if not $expect_id or ((not ($token | str starts-with '@with:')) and ($token !~ '^[A-Za-z0-9][A-Za-z0-9.+-]*$')) {
                return {valid: false, allowed: false, selected_ids: []}
            }
            $output = ($output | append $token)
            $expect_id = false
        }
    }
    if $expect_id { return {valid: false, allowed: false, selected_ids: []} }
    while not ($operators | is-empty) {
        let operator = ($operators | last)
        if $operator == '(' { return {valid: false, allowed: false, selected_ids: []} }
        $output = ($output | append $operator)
        $operators = ($operators | drop 1)
    }

    mut values = []
    for token in $output {
        if $token in [AND OR] {
            if ($values | length) < 2 { return {valid: false, allowed: false, selected_ids: []} }
            let right = ($values | last)
            $values = ($values | drop 1)
            let left = ($values | last)
            $values = ($values | drop 1)
            if $token == 'AND' {
                $values = ($values | append {
                    allowed: ($left.allowed and $right.allowed)
                    selected_ids: (if $left.allowed and $right.allowed { $left.selected_ids | append $right.selected_ids | uniq } else { [] })
                })
            } else {
                let left_notices = ($left.selected_ids | where $it in $notice_atoms | length)
                let right_notices = ($right.selected_ids | where $it in $notice_atoms | length)
                let chosen = if $left.allowed and (not $right.allowed or $left_notices <= $right_notices) { $left } else { $right }
                $values = ($values | append $chosen)
            }
        } else {
            let atom = if ($token | str starts-with '@with:') {
                let parts = ($token | split row ':')
                $'($parts.1) WITH ($parts.2)'
            } else { $token }
            let allowed = ($atom in $approved_atoms)
            $values = ($values | append {allowed: $allowed, selected_ids: (if $allowed { [$atom] } else { [] })})
        }
    }
    if ($values | length) != 1 { return {valid: false, allowed: false, selected_ids: []} }
    let result = ($values | first)
    {valid: true, allowed: $result.allowed, selected_ids: $result.selected_ids}
}

def main [metadata_path: string, target: string, profile: string, output_path: string] {
    if $profile not-in [default tauri-sounds] {
        error make {msg: $'Unknown feature profile: ($profile)'}
    }
    if $target == 'wasm32-unknown-unknown' and $profile != 'default' {
        error make {msg: 'The wasm32-unknown-unknown inventory only supports the default feature profile'}
    }
    # The hosted artifact retains these exact raw bytes. Bind the derived
    # inventory to both the filtered resolve graph and the workspace lockfile.
    let metadata_sha256 = (^sha256sum $metadata_path | split row ' ' | first)
    let cargo_lock_sha256 = (^sha256sum Cargo.lock | split row ' ' | first)
    let approval_manifest_sha256 = (^sha256sum scripts/license_approvals.json | split row ' ' | first)
    if ($metadata_sha256 | str length) != 64 or ($cargo_lock_sha256 | str length) != 64 or ($approval_manifest_sha256 | str length) != 64 {
        error make {msg: 'Could not bind inventory to Cargo metadata, Cargo.lock, and approval manifest SHA-256'}
    }
    let metadata = (open --raw $metadata_path | from json)
    let current_profile = $'($target)/($profile)'
    let manifest = (open --raw 'scripts/license_approvals.json' | from json)
    if $manifest.schema_version != 1 { error make {msg: 'Unsupported license approval manifest schema'} }
    let approvals = $manifest.approvals
    let lock_packages = (open --raw 'Cargo.lock' | from toml | get package)
    mut seen_approval_profiles = []
    mut seen_approval_ids = []
    for approval in $approvals {
        if $approval.id in $seen_approval_ids {
            error make {msg: $'Duplicate license approval ID: ($approval.id)'}
        }
        $seen_approval_ids = ($seen_approval_ids | append $approval.id)
        if $approval.package_id != $'($approval.source)#($approval.name)@($approval.version)' {
            error make {msg: $'Invalid exact package ID in license approval: ($approval.id)'}
        }
        if ($approval.decision_ref | str trim | is-empty) or ($approval.approved_choice | is-empty) or ($approval.profiles | is-empty) {
            error make {msg: $'Incomplete license approval: ($approval.id)'}
        }
        if ($approval.approved_choice | uniq | length) != ($approval.approved_choice | length) {
            error make {msg: $'Duplicate choice atom in license approval: ($approval.id)'}
        }
        let expected_notices = ($approval.approved_choice | where $it not-in $base_allowed | sort)
        if $expected_notices != ($approval.notice_ids | sort) {
            error make {msg: $'Choice/notice mismatch in license approval: ($approval.id)'}
        }
        let locked = ($lock_packages | where {|item| $item.name == $approval.name and $item.version == $approval.version and $item.source == $approval.source})
        if ($locked | length) != 1 or (($locked | first).checksum != $approval.lock_checksum) {
            error make {msg: $'Cargo.lock checksum or tuple mismatch for license approval: ($approval.id)'}
        }
        for approved_profile in $approval.profiles {
            if $approved_profile not-in $inventory_profiles {
                error make {msg: $'Unsupported profile in license approval: ($approval.id)'}
            }
            let key = $'($approval.name)@($approval.version)|($approval.source)|($approved_profile)'
            if $key in $seen_approval_profiles {
                error make {msg: $'Duplicate license approval selector: ($key)'}
            }
            $seen_approval_profiles = ($seen_approval_profiles | append $key)
        }
    }
    let roots = (roots_for $target)
    let root_ids = ($metadata.packages | where name in $roots | get id)
    if ($root_ids | length) != ($roots | length) {
        error make {msg: $'Expected exactly ($roots | length) shipped roots for ($target); found ($root_ids | length)'}
    }

    mut queue = ($root_ids | each {|id| {id: $id, role: 'product'}})
    mut shipped_ids = []
    mut build_ids = []
    mut dropped_optional_edges = []
    while not ($queue | is-empty) {
        let item = ($queue | first)
        $queue = ($queue | skip 1)
        let id = $item.id
        if $item.role == 'product' {
            if $id in $shipped_ids { continue }
            $shipped_ids = ($shipped_ids | append $id)
        } else {
            if $id in $build_ids { continue }
            $build_ids = ($build_ids | append $id)
        }
        let matches = ($metadata.resolve.nodes | where id == $id)
        if ($matches | length) != 1 {
            error make {msg: $'Missing or duplicate resolve node: ($id)'}
        }
        let node = ($matches | first)
        let parent_packages = ($metadata.packages | where id == $id)
        if ($parent_packages | length) != 1 {
            error make {msg: $'Missing or duplicate package metadata: ($id)'}
        }
        let parent_package = ($parent_packages | first)
        for dependency in $node.deps {
            let live_kinds = ($dependency.dep_kinds | where kind != 'dev')
            if not ($live_kinds | is-empty) {
                let target_packages = ($metadata.packages | where id == $dependency.pkg)
                if ($target_packages | length) != 1 {
                    error make {msg: $'Missing or duplicate dependency package metadata: ($dependency.pkg)'}
                }
                let target_package = ($target_packages | first)
                # Cargo reports proc-macro dependencies as normal edges, but
                # proc-macro crates execute on the build host. Their own
                # dependencies are host-side as well. Only explicitly known
                # nonproduct target kinds may accompany proc-macro; an unknown
                # or runtime target keeps the product role conservatively.
                let target_kinds = ($target_package.targets | each {|target| $target.kind} | flatten)
                let has_proc_macro = ('proc-macro' in $target_kinds)
                let only_host_target_kinds = ($target_kinds | all {|target_kind| $target_kind in ['proc-macro' 'custom-build' 'test' 'bench' 'example']})
                let host_only_proc_macro = $has_proc_macro and $only_host_target_kinds
                let declarations = ($parent_package.dependencies | where {|declared|
                    let alias_matches = if $declared.rename == null { true } else { ($declared.rename | str replace --all '-' '_') == $dependency.name }
                    $declared.name == $target_package.name and $declared.source == $target_package.source and $alias_matches and $declared.kind != 'dev'
                })
                # Only one exact package/source declaration can prove inactivity.
                # Unknown or ambiguous mappings remain included conservatively.
                mut active = (($declarations | length) != 1)
                if ($declarations | length) == 1 {
                    let declared = ($declarations | first)
                    let key = if $declared.rename == null { $declared.name } else { $declared.rename }
                    if (not $declared.optional) or (optional_dependency_active $parent_package $node $key) {
                        $active = true
                    } else {
                        $dropped_optional_edges = ($dropped_optional_edges | append {
                            parent_id: $id
                            dependency_id: $dependency.pkg
                            dependency_key: $key
                            target_profile: $current_profile
                            parent_features: $node.features
                        })
                    }
                }
                if $active {
                    for kind in $live_kinds {
                        let next_role = if $item.role == 'build-only' or $kind.kind == 'build' or $host_only_proc_macro { 'build-only' } else { 'product' }
                        $queue = ($queue | append {id: $dependency.pkg, role: $next_role})
                    }
                }
            }
        }
    }

    mut rows = []
    mut used_approval_ids = []
    for package in $metadata.packages {
        let shipped = ($package.id in $shipped_ids)
        let build_only = (($package.id in $build_ids) and (not $shipped))
        # Only these reviewed workspace members qualify as first-party private.
        # An arbitrary path dependency cannot bypass the product license gate.
        let first_party = ($package.id in $metadata.workspace_members and $package.name in $reviewed_workspace_names and $package.source == null and ($package.manifest_path | str starts-with $'($metadata.workspace_root)/'))
        let global_atoms = ($base_allowed | append $approved_extra)
        let global_result = if $package.license == null {
            {valid: false, allowed: false, selected_ids: []}
        } else {
            license_evaluation $package.license $global_atoms $approved_extra
        }
        let matching = ($approvals | where {|item|
            $item.name == $package.name and $item.version == $package.version and $item.source == $package.source and $item.package_id == $package.id and $item.declared_license == $package.license and $current_profile in $item.profiles
        })
        mut parsed = $global_result
        mut approval_id = null
        if $shipped and (not $first_party) and (not $global_result.allowed) and (($matching | length) == 1) {
            let approval = ($matching | first)
            let exact_atoms = ($global_atoms | append $approval.approved_choice | uniq)
            let notice_atoms = ($exact_atoms | where $it not-in $base_allowed)
            let exact_result = (license_evaluation $package.license $exact_atoms $notice_atoms)
            let exact_notices = ($exact_result.selected_ids | where $it not-in $base_allowed | sort)
            if $exact_result.allowed and (($exact_result.selected_ids | sort) == ($approval.approved_choice | sort)) and ($exact_notices == ($approval.notice_ids | sort)) {
                $parsed = $exact_result
                $approval_id = $approval.id
                $used_approval_ids = ($used_approval_ids | append $approval.id)
            }
        }
        let needs_notice = ($parsed.selected_ids | where $it not-in $base_allowed | uniq)
        let policy = if not $shipped { 'not-applicable' } else if $first_party { 'first-party-private' } else if $approval_id != null { 'approved-exact' } else if $global_result.allowed { 'allowed' } else { 'review-required' }
        $rows = ($rows | append {
            name: $package.name
            version: $package.version
            source: $package.source
            manifest_path: $package.manifest_path
            license: $package.license
            legacy_slash_alternatives: (if $package.license == null { false } else { $package.license | str contains '/' })
            selected_license_ids: $parsed.selected_ids
            approval_id: $approval_id
            first_party: $first_party
            role: (if $shipped { 'product' } else if $build_only { 'build-only' } else { 'tool-only-or-unselected' })
            product_policy: $policy
            notice_ids: $needs_notice
        })
    }
    let review = ($rows | where product_policy == 'review-required')
    let expected_approval_ids = ($approvals | where {|item| $current_profile in $item.profiles} | get id)
    let unused_approval_ids = ($expected_approval_ids | where $it not-in $used_approval_ids)
    let result = {
        target: $target
        roots: $roots
        feature_profile: $profile
        cargo_feature_args: (if $profile == 'tauri-sounds' { ['--features' 'at-tauri/sounds'] } else { [] })
        cargo_metadata_sha256: $metadata_sha256
        cargo_lock_sha256: $cargo_lock_sha256
        approval_manifest_sha256: $approval_manifest_sha256
        approval_manifest_schema: $manifest.schema_version
        used_approval_ids: ($used_approval_ids | sort)
        unused_approval_ids: ($unused_approval_ids | sort)
        dropped_inactive_optional_edges: $dropped_optional_edges
        product_count: ($rows | where role == 'product' | length)
        build_only_count: ($rows | where role == 'build-only' | length)
        tool_or_unselected_count: ($rows | where role == 'tool-only-or-unselected' | length)
        review_required_count: ($review | length)
        packages: ($rows | sort-by role name version)
    }
    $result | to json --indent 2 | save --force $output_path
    print $'Target ($target): ($result.product_count) product packages, ($result.build_only_count) build-only packages, ($result.tool_or_unselected_count) other unselected; ($result.review_required_count) product license reviews required; ($unused_approval_ids | length) stale approvals.'
    for item in $review { print $'REVIEW ($item.name)@($item.version): ($item.license)' }
    for id in $unused_approval_ids { print $'STALE APPROVAL ($id)' }
    if (not ($review | is-empty)) or (not ($unused_approval_ids | is-empty)) { exit 1 }
}
