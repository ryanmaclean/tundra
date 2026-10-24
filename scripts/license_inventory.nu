#!/usr/bin/env nu

# CI-only, locked Cargo metadata classifier. Build dependencies are treated as
# product dependencies because generated code can be incorporated into output.
const base_allowed = [MIT BSD-2-Clause BSD-3-Clause Apache-2.0]
const approved_extra = [ISC CDLA-Permissive-2.0 Unicode-3.0]
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

def precedence [operator: string] {
    if $operator == 'AND' { 2 } else { 1 }
}

def license_evaluation [expression: string] {
    # A legacy slash is only accepted for a plain list of alternatives.
    # Mixing it with SPDX operators or grouping has ambiguous precedence;
    # fail closed rather than turning a mandatory term into an optional one.
    let legacy_slash = ($expression | str contains '/')
    if $legacy_slash and (
        ($expression | str contains '(')
        or ($expression | str contains ')')
        or ($expression =~ '\b(AND|OR|WITH)\b')
    ) {
        return {valid: false, allowed: false, selected_ids: []}
    }
    # Tokenize unambiguous legacy alternatives as OR. An adjacent ID WITH
    # exception-ID becomes one deliberately unapproved atom. Reject malformed
    # or repeated WITH before evaluating any independent OR alternative.
    let raw_tokens = ($expression | str replace --all '/' ' OR ' | str replace --all '(' ' ( ' | str replace --all ')' ' ) ' | str replace --all --regex '\s+' ' ' | str trim | split row ' ' | where $it != '')
    if ($raw_tokens | is-empty) or ('@with' in $raw_tokens) {
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
            $tokens = ($tokens | append '@with')
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
            if not $expect_id or (($token != '@with') and ($token !~ '^[A-Za-z0-9][A-Za-z0-9.+-]*$')) {
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

    let approved = ($base_allowed | append $approved_extra)
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
                let left_notices = ($left.selected_ids | where $it in $approved_extra | length)
                let right_notices = ($right.selected_ids | where $it in $approved_extra | length)
                let chosen = if $left.allowed and (not $right.allowed or $left_notices <= $right_notices) { $left } else { $right }
                $values = ($values | append $chosen)
            }
        } else {
            let allowed = ($token != '@with' and $token in $approved)
            $values = ($values | append {allowed: $allowed, selected_ids: (if $allowed { [$token] } else { [] })})
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
    if ($metadata_sha256 | str length) != 64 or ($cargo_lock_sha256 | str length) != 64 {
        error make {msg: 'Could not bind inventory to Cargo metadata and Cargo.lock SHA-256'}
    }
    let metadata = (open --raw $metadata_path | from json)
    let roots = (roots_for $target)
    let root_ids = ($metadata.packages | where name in $roots | get id)
    if ($root_ids | length) != ($roots | length) {
        error make {msg: $'Expected exactly ($roots | length) shipped roots for ($target); found ($root_ids | length)'}
    }

    mut queue = $root_ids
    mut shipped_ids = []
    while not ($queue | is-empty) {
        let id = ($queue | first)
        $queue = ($queue | skip 1)
        if $id in $shipped_ids { continue }
        $shipped_ids = ($shipped_ids | append $id)
        let matches = ($metadata.resolve.nodes | where id == $id)
        if ($matches | length) != 1 {
            error make {msg: $'Missing or duplicate resolve node: ($id)'}
        }
        let node = ($matches | first)
        for dependency in $node.deps {
            let live_kinds = ($dependency.dep_kinds | where kind != 'dev')
            if not ($live_kinds | is-empty) {
                $queue = ($queue | append $dependency.pkg)
            }
        }
    }

    mut rows = []
    for package in $metadata.packages {
        let shipped = ($package.id in $shipped_ids)
        # Only these reviewed workspace members qualify as first-party private.
        # An arbitrary path dependency cannot bypass the product license gate.
        let first_party = (
            $package.id in $metadata.workspace_members
            and $package.name in $reviewed_workspace_names
            and $package.source == null
            and ($package.manifest_path | str starts-with $'($metadata.workspace_root)/')
        )
        let parsed = if $package.license == null { {valid: false, allowed: false, selected_ids: []} } else { license_evaluation $package.license }
        let needs_notice = ($parsed.selected_ids | where $it in $approved_extra | uniq)
        let accepted = ($parsed.valid and $parsed.allowed)
        $rows = ($rows | append {
            name: $package.name
            version: $package.version
            source: $package.source
            manifest_path: $package.manifest_path
            license: $package.license
            legacy_slash_alternatives: (if $package.license == null { false } else { $package.license | str contains '/' })
            selected_license_ids: $parsed.selected_ids
            first_party: $first_party
            role: (if $shipped { 'product' } else { 'tool-only-or-unselected' })
            product_policy: (if not $shipped { 'not-applicable' } else if $first_party { 'first-party-private' } else if $accepted { 'allowed' } else { 'review-required' })
            notice_ids: $needs_notice
        })
    }
    let review = ($rows | where product_policy == 'review-required')
    let result = {
        target: $target
        roots: $roots
        feature_profile: $profile
        cargo_feature_args: (if $profile == 'tauri-sounds' { ['--features' 'at-tauri/sounds'] } else { [] })
        cargo_metadata_sha256: $metadata_sha256
        cargo_lock_sha256: $cargo_lock_sha256
        product_count: ($rows | where role == 'product' | length)
        tool_or_unselected_count: ($rows | where role == 'tool-only-or-unselected' | length)
        review_required_count: ($review | length)
        packages: ($rows | sort-by role name version)
    }
    $result | to json --indent 2 | save --force $output_path
    print $'Target ($target): ($result.product_count) product packages, ($result.tool_or_unselected_count) tool-only/unselected; ($result.review_required_count) product license reviews required.'
    for item in $review { print $'REVIEW ($item.name)@($item.version): ($item.license)' }
    if not ($review | is-empty) { exit 1 }
}
