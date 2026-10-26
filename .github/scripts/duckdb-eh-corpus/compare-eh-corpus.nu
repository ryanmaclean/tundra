#!/usr/bin/env nu
# Exact-run downloaded receipts only. Missing/truncated inputs fail closed.

def verify_artifacts [root: path] {
  let manifest = (open ($root | path join 'artifact-auth.json'))
  let names = ['duckdb-eh-pair-image-default' 'duckdb-eh-pair-image-sounds'
    'duckdb-eh-pair-clean-default' 'duckdb-eh-pair-clean-sounds'
    'duckdb-eh-old-default' 'duckdb-eh-old-sounds']
  if ($manifest.schema != 'tundra-eh-artifact-auth/v1' or $manifest.admission != 'NO' or
     $manifest.repo != 'ryanmaclean/tundra' or $manifest.run_id <= 0 or
     $manifest.head_sha !~ '^[0-9a-f]{40}$' or
     ($manifest.artifacts | length) != 6 or
     (($manifest.artifacts | get name | sort) != ($names | sort))) {
    error make {msg: 'exact-run artifact authentication manifest invalid'}
  }
  let ids = ($manifest.artifacts | get id)
  if ($ids | uniq | length) != 6 { error make {msg: 'duplicate artifact IDs'} }
  for row in $manifest.artifacts {
    if ($row.id <= 0 or $row.run_id != $manifest.run_id or
       $row.head_sha != $manifest.head_sha or
       $row.zip_sha256 !~ '^[0-9a-f]{64}$' or
       $row.api_digest != $"sha256:($row.zip_sha256)" or
       $row.size_in_bytes <= 0) {
      error make {msg: $"artifact identity/digest join invalid: ($row.name)"}
    }
    let zip_path = ($root | path join 'artifact-zips' $"($row.name).zip")
    let actual_zip_sha = (^sha256sum $zip_path | split row ' ' | first)
    if $actual_zip_sha != $row.zip_sha256 {
      error make {msg: $"retained artifact ZIP digest mismatch: ($row.name)"}
    }
    let directory = ($root | path join $row.name)
    let keys = ($row.member_sha256 | columns)
    if ($keys | length) == 0 { error make {msg: $"empty artifact: ($row.name)"} }
    for key in $keys {
      if (($key | str starts-with '/') or ($key | str contains '\\') or
          ('..' in ($key | split row '/')) or ('.' in ($key | split row '/'))) {
        error make {msg: $"unsafe member path: ($row.name)"}
      }
      let wanted = ($row.member_sha256 | get $key)
      let file = ($directory | path join $key)
      let actual = (^sha256sum $file | split row ' ' | first)
      if $wanted !~ '^[0-9a-f]{64}$' or $actual != $wanted {
        error make {msg: $"extracted member digest mismatch: ($row.name)/($key)"}
      }
    }
    let found = (^find $directory -type f | lines | each {|file|
      $file | str replace $"($directory)/" ''} | sort)
    if $found != ($keys | sort) {
      error make {msg: $"extracted member set mismatch: ($row.name)"}
    }
  }
  $manifest
}

def inspect_corpus [file: path, expected_ids: list<string>] {
  let receipt = (open $file)
  let corpus = ($receipt.result.corpusObserved | from json)
  if ($receipt.ok != true or $receipt.result.ok != true or
     $receipt.result.exceptionOk != true or
     $receipt.result.cspDenied != true or $receipt.result.localRuntime != true or
     $receipt.result.remoteRequests != 0 or ($receipt.result.cspViolations | length) != 0) {
    error make {msg: $"offline/CSP smoke failed: ($file)"}
  }
  if ($corpus.schema != 'tundra-installed-eh-corpus/v1' or
     $corpus.directConnection != 'SEPARATE_SAME_INSTALLED_PAIR') {
    error make {msg: $"invalid corpus or connection label: ($file)"}
  }
  let unsupported = ['arrow-positive-ipc' 'multibatch-stream' 'upstream-full-suite']
  if ((($corpus.unsupported | sort) != ($unsupported | sort)) or
     (($corpus.skipped | to json --raw) != '[]')) {
    error make {msg: $"unsupported/skip set changed: ($file)"}
  }
  let ids = ($corpus.cases | get id)
  if (($ids | length) != ($expected_ids | length) or
     ($ids | uniq | length) != ($expected_ids | length) or
     (($ids | sort) != ($expected_ids | sort))) {
    error make {msg: $"case set omitted, duplicated or changed: ($file)"}
  }
  if $corpus.passed != ($corpus.cases | all {|case| $case.passed == true}) {
    error make {msg: $"corpus verdict inconsistent with cases: ($file)"}
  }
  let error_classes = {bridge-parser-error: 'Parser', bridge-catalog-error: 'Catalog',
    bridge-binder-error: 'Binder', bridge-conversion-error: 'Conversion',
    direct-file-error: 'IO', direct-arrow-invalid-ipc: 'Invalid Input'}
  let canonical = ($corpus.cases | each {|case|
    if (($case.passed != true and $case.passed != false) or
       (($case.skipped? | default false) != false) or
       (($case.unsupported? | default false) != false)) {
      error make {msg: $"invalid/skipped case ($case.id) in ($file)"}
    }
    if $case.id in ($error_classes | columns) {
      let wanted = ($error_classes | get $case.id)
      if ($case.passed == true and
          not (($case.errorClass | str contains $wanted) and (($case.marker | str length) > 0))) {
        error make {msg: $"wrong error class/marker for ($case.id): ($file)"}
      }
    } else if ($case.passed == true and $case.id != 'installed-local-pair-urls') {
      if ($case.canonicalSha256 !~ '^[0-9a-f]{64}$') or $case.canonicalBytes <= 0 {
        error make {msg: $"missing full-output digest for ($case.id): ($file)"}
      }
    }
    let diagnostic = ($case.error? | default '')
    if (($diagnostic | str length) > 160) {
      error make {msg: $"unbounded case diagnostic ($case.id): ($file)"}
    }
    {id: $case.id, phase: $case.phase, passed: $case.passed,
      canonicalSha256: ($case.canonicalSha256? | default ''),
      canonicalBytes: ($case.canonicalBytes? | default 0),
      errorClass: ($case.errorClass? | default ''), marker: ($case.marker? | default ''),
      error: $diagnostic,
      responseShape: ($case.responseShape? | default ''),
      responseDigest: ($case.responseDigest? | default null),
      responseLength: ($case.responseLength? | default null),
      rejectionStage: ($case.rejectionStage? | default '')}
  })
  let selected = $corpus.selected
  if not (($selected.moduleUrl | str ends-with '/duckdb-runtime/duckdb-browser.mjs') and
          ($selected.workerUrl | str ends-with '/duckdb-runtime/duckdb-browser-eh.worker.js') and
          ($selected.wasmUrl | str ends-with '/duckdb-runtime/duckdb-eh.wasm')) {
    error make {msg: $"selected local URLs are wrong: ($file)"}
  }
  for key in ['moduleSha256' 'workerSha256' 'wasmSha256'] {
    if (($selected | get $key) !~ '^[0-9a-f]{64}$') {
      error make {msg: $"selected asset hash missing ($key): ($file)"}
    }
  }
  if ($corpus.fixtures.csv.sha256 != '49bf74f9f299168417c98d9d0a61d3ce8a8ede696d089e6d563157fddda7710f' or
     $corpus.fixtures.csv.bytes != 24 or
     $corpus.fixtures.json.sha256 != '8471cd08902c06d1d2be13aacd9b5f9ae086d71446a275d2bf6386454eceb46e' or
     $corpus.fixtures.json.bytes != 34) {
    error make {msg: $"literal fixture hashes changed: ($file)"}
  }
  let nonsemantic = ['installed-local-pair-urls' 'installed-local-pair-bytes'
    'direct-parquet-buffer' 'direct-parquet-reregister']
  let semantic = ($canonical | where {|case| $case.id not-in $nonsemantic} |
    each {|case| {id: $case.id, phase: $case.phase, passed: $case.passed,
      canonicalSha256: $case.canonicalSha256, canonicalBytes: $case.canonicalBytes,
      errorClass: $case.errorClass}})
  {receipt: $receipt, selected: $selected, cases: $canonical, semantic: $semantic}
}

def case_signature [row: record] {
  let nonsemantic = ['installed-local-pair-urls' 'installed-local-pair-bytes'
    'direct-parquet-buffer' 'direct-parquet-reregister']
  if $row.passed == false {
    return {phase: $row.phase, passed: false,
      canonicalSha256: $row.canonicalSha256, canonicalBytes: $row.canonicalBytes,
      errorClass: $row.errorClass, marker: $row.marker, error: $row.error,
      responseShape: $row.responseShape, responseDigest: $row.responseDigest,
      responseLength: $row.responseLength, rejectionStage: $row.rejectionStage}
  }
  if $row.id in $nonsemantic {
    {phase: $row.phase, passed: $row.passed}
  } else {
    {phase: $row.phase, passed: $row.passed,
      canonicalSha256: $row.canonicalSha256, canonicalBytes: $row.canonicalBytes,
      errorClass: $row.errorClass}
  }
}

def main [receipt_root: path] {
  let artifact_auth = (verify_artifacts $receipt_root)
  let expected_ids = [bridge-create bridge-json-insert bridge-empty-insert bridge-persist-null-utf8
    bridge-macro-create bridge-macro-query bridge-parser-error bridge-parser-recovery
    bridge-catalog-error bridge-catalog-recovery bridge-binder-error bridge-conversion-error
    bridge-after-errors bridge-arrow-json-types bridge-large-bigint-string
    installed-local-pair-urls installed-local-pair-bytes direct-arrow-schema
    direct-csv-register direct-csv-scan direct-csv-insert direct-json-register
    direct-json-scan direct-json-insert direct-parquet-copy direct-parquet-buffer
    direct-parquet-reregister direct-parquet-scan direct-file-error direct-after-error
    direct-arrow-invalid-ipc direct-after-arrow-error]
  let cells = ['image-default' 'image-sounds' 'clean-default' 'clean-sounds']
  let snapshots = ($cells | each {|cell|
    let directory = ($receipt_root | path join $"duckdb-eh-pair-($cell)")
    let item = (inspect_corpus ($directory | path join 'webkit.json') $expected_ids)
    let custody = (open ($directory | path join 'installed-legal-custody.json'))
    let pair = (open ($directory | path join 'pair.json'))
    let assets = $custody.installed_assets
    let profile = ($cell | split row '-' | last)
    let condition = ($cell | split row '-' | first)
    if ($item.receipt.head_sha != 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' or
       $item.receipt.head_sha != $custody.consumer_head or
       $item.receipt.profile != $profile or $custody.profile != $profile or
       $custody.condition != $condition or
       $custody.producer_head != 'fbcc2be993b9d47fd062d7833cf52f4a630649f8' or
       $item.receipt.requested_variant != 'eh' or $item.receipt.result.variant != 'eh' or
       $custody.installer_sha256 !~ '^[0-9a-f]{64}$') {
      error make {msg: $"new-cell head/profile/condition custody mismatch: ($cell)"}
    }
    if ($item.selected.moduleSha256 != ($assets | get 'duckdb-runtime/duckdb-browser.mjs') or
       $item.selected.workerSha256 != ($assets | get 'duckdb-runtime/duckdb-browser-eh.worker.js') or
       $item.selected.workerSha256 != $pair.worker_sha256 or
       $item.selected.wasmSha256 != ($assets | get 'duckdb-runtime/duckdb-eh.wasm') or
       $item.receipt.installed_elf_sha256 != $custody.installed_elf_sha256) {
      error make {msg: $"selected bytes do not join installed custody: ($cell)"}
    }
    let environment = (open ($directory | path join 'corpus-environment.json'))
    if ($environment.schema != 'tundra-installed-eh-corpus-environment/v1' or
       $environment.profile != ($cell | split row '-' | last) or
       $environment.harness_sha256 !~ '^[0-9a-f]{64}$' or
       (($environment.webkitgtk_version | str length) == 0) or
       $environment.offline != true or $environment.csp_enforced != true) {
      error make {msg: $"environment/harness provenance missing: ($cell)"}
    }
    {cell: $cell, cases: $item.cases, semantic: $item.semantic,
      selected: $item.selected,
      environment: $environment, installer_sha256: $custody.installer_sha256,
      installed_elf_sha256: $custody.installed_elf_sha256}
  })
  let baselines = (['default' 'sounds'] | each {|profile|
    let directory = ($receipt_root | path join $"duckdb-eh-old-($profile)")
    let item = (inspect_corpus ($directory | path join 'webkit.json') $expected_ids)
    let provenance = (open ($directory | path join 'old-baseline-provenance.json'))
    if ($provenance.schema != 'tundra-old-installed-eh-baseline/v1' or
       $provenance.consumer_head != 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' or
       $provenance.source_checkout_head != $provenance.consumer_head or
       $provenance.installed_pr119_bill_sha256 != '34381cba60c8922a159f578753b55531460792372f2d72b22769da205fe97790' or
       $item.receipt.head_sha != $provenance.consumer_head or
       $provenance.profile != $profile or $item.receipt.profile != $profile or
       $item.receipt.requested_variant != 'eh' or $item.receipt.result.variant != 'eh' or
       $item.receipt.installed_elf_sha256 != $provenance.installed_elf_sha256 or
       $item.selected.moduleSha256 != '8ccf3fe3c6bd87d5c3e397f01d40cc11ea6373b016a9cf5e954d39ee22c3aa49' or
       $item.selected.workerSha256 != 'feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4' or
       $item.selected.wasmSha256 != 'd71b7e4575c98b87e09ac799c574d770aa5ee4df5e6c6cf36b6e277d92b8d2c2' or
       $item.selected.moduleSha256 != ($provenance.installed_assets | get 'duckdb-runtime/duckdb-browser.mjs') or
       $item.selected.workerSha256 != ($provenance.installed_assets | get 'duckdb-runtime/duckdb-browser-eh.worker.js') or
       $item.selected.wasmSha256 != ($provenance.installed_assets | get 'duckdb-runtime/duckdb-eh.wasm') or
       $provenance.installer_sha256 !~ '^[0-9a-f]{64}$' or
       $provenance.installed_elf_sha256 !~ '^[0-9a-f]{64}$' or
       $provenance.harness_sha256 !~ '^[0-9a-f]{64}$' or
       (($provenance.webkitgtk_version | str length) == 0) or
       $provenance.offline != true or $provenance.csp_enforced != true) {
      error make {msg: $"old installed baseline provenance unbound: ($profile)"}
    }
    {profile: $profile, cases: $item.cases, semantic: $item.semantic,
      selected: $item.selected, provenance: $provenance}
  })
  let reference = ($snapshots.0.semantic | to json --raw)
  let new_differences = ($snapshots | where {|item| ($item.semantic | to json --raw) != $reference} | get cell)
  let old_differences = ($baselines | where {|item| ($item.semantic | to json --raw) != $reference} | get profile)
  let webkit_versions = ($snapshots | get environment.webkitgtk_version | uniq)
  let harness_hashes = ($snapshots | get environment.harness_sha256 | uniq)
  if ($webkit_versions | length) != 1 or ($harness_hashes | length) != 1 {
    error make {msg: 'new-cell WebKitGTK or harness source drift'}
  }
  for baseline in $baselines {
    if ($baseline.provenance.webkitgtk_version != $webkit_versions.0 or
       $baseline.provenance.harness_sha256 != $harness_hashes.0) {
      error make {msg: $"baseline environment/harness drift: ($baseline.profile)"}
    }
  }
  let case_comparisons = ($expected_ids | each {|id|
    let new = ($snapshots | each {|cell|
      let row = ($cell.cases | where id == $id | first)
      {cell: $cell.cell, passed: $row.passed, signature: (case_signature $row)}
    })
    let old = ($baselines | each {|cell|
      let row = ($cell.cases | where id == $id | first)
      {profile: $cell.profile, passed: $row.passed, signature: (case_signature $row)}
    })
    let signatures = (($new | get signature) ++ ($old | get signature) |
      each {|value| $value | to json --raw} | uniq)
    let failures = (($new | where passed == false | get cell) ++
      ($old | where passed == false | get profile | each {|profile| $"old-($profile)"}))
    {id: $id, new: $new, old: $old, equal: (($signatures | length) == 1),
      all_passed: (($failures | length) == 0), failed_cells: $failures}
  })
  let case_failures = ($case_comparisons | where all_passed == false | get id)
  let case_differences = ($case_comparisons | where equal == false | get id)
  let equal = ((($new_differences | length) == 0) and
    (($old_differences | length) == 0) and
    (($case_failures | length) == 0) and (($case_differences | length) == 0))
  let result = {schema: 'tundra-installed-eh-corpus-compare/v2', admission: 'NO',
    artifact_auth: $artifact_auth,
    case_count: ($expected_ids | length), equal: $equal,
    new_differences: $new_differences, old_differences: $old_differences,
    case_failures: $case_failures, case_differences: $case_differences,
    case_comparisons: $case_comparisons,
    snapshots: $snapshots, baselines: $baselines}
  $result | to json --raw | save --force ($receipt_root | path join 'corpus-comparison.json')
  if not $equal { error make {msg: 'installed EH corpus case failure or difference'} }
}
