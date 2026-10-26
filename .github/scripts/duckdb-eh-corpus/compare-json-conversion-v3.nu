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
    direct-file-error: 'IO'}
  let canonical = ($corpus.cases | each {|case|
    if (($case.passed != true and $case.passed != false) or
       (($case.skipped? | default false) != false) or
       (($case.unsupported? | default false) != false)) {
      error make {msg: $"invalid/skipped case ($case.id) in ($file)"}
    }
    if $case.id == 'direct-arrow-invalid-ipc' and $case.passed == true {
      if (($case.errorClass? | default '') != 'Error' or
          ($case.marker? | default '') != 'Error: Invalid IPC message: negative metadata length' or
          (($case.marker? | default '') | str length) > 120 or
          ($case.fixtureSha256? | default '') != '12a3ae445661ce5dee78d0650d33362dec29c4f82af05e7e57fb595bbbacf0ca' or
          ($case.fixtureBytes? | default 0) != 8 or
          ($case.rejectionStage? | default '') != 'registration' or
          ($case.absent? | default false) != true or
          ($case.absenceClass? | default '') != 'Catalog Error' or
          ($case.recovered? | default false) != true) {
        error make {msg: $"invalid pinned Arrow rejection/absence/recovery: ($file)"}
      }
    } else if $case.id in ($error_classes | columns) {
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
      rejectionStage: ($case.rejectionStage? | default ''),
      fixtureSha256: ($case.fixtureSha256? | default ''),
      fixtureBytes: ($case.fixtureBytes? | default 0),
      absent: ($case.absent? | default false),
      absenceClass: ($case.absenceClass? | default ''),
      recovered: ($case.recovered? | default false)}
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
  {receipt: $receipt, selected: $selected, cases: $canonical,
    semantic: $semantic, conversion: $corpus.conversionDiagnostics}
}

def case_signature [row: record] {
  let nonsemantic = ['installed-local-pair-urls' 'installed-local-pair-bytes'
    'direct-parquet-buffer' 'direct-parquet-reregister']
  if $row.passed == false {
    return {phase: $row.phase, passed: false,
      canonicalSha256: $row.canonicalSha256, canonicalBytes: $row.canonicalBytes,
      errorClass: $row.errorClass, marker: $row.marker, error: $row.error,
      responseShape: $row.responseShape, responseDigest: $row.responseDigest,
      responseLength: $row.responseLength, rejectionStage: $row.rejectionStage,
      fixtureSha256: $row.fixtureSha256, fixtureBytes: $row.fixtureBytes,
      absent: $row.absent, absenceClass: $row.absenceClass,
      recovered: $row.recovered}
  }
  if $row.id in $nonsemantic {
    {phase: $row.phase, passed: $row.passed}
  } else if $row.id == 'direct-arrow-invalid-ipc' {
    {phase: $row.phase, passed: $row.passed,
      errorClass: $row.errorClass, marker: $row.marker,
      rejectionStage: $row.rejectionStage,
      fixtureSha256: $row.fixtureSha256, fixtureBytes: $row.fixtureBytes,
      absent: $row.absent, absenceClass: $row.absenceClass,
      recovered: $row.recovered}
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
    let direct = $item.conversion.direct
    if ($direct.status != 'PROBED' or $direct.readback.matches != true or
        $direct.query.stage != 'query-rejected' or
        ($direct.query.errorClass | str contains 'Conversion') != true or
        $direct.rawRunQuery.stage != 'raw-rejected' or
        ($direct.rawRunQuery.errorClass | str contains 'Conversion') != true or
        $direct.recovered != true) {
      error make {msg: $"new direct Conversion rejection/readback/recovery missing: ($cell)"}
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
    let direct = $item.conversion.direct
    if ($direct.status != 'PROBED' or $direct.readback.matches != true or
        $direct.query.stage != 'query-resolved' or $direct.query.numRows != 0 or
        $direct.rawRunQuery.stage != 'raw-resolved' or
        $direct.rawRunQuery.digest.bytes != 314 or
        $direct.rawRunQuery.digest.sha256 != 'a6f59793c2392a4fad3fc3000def500eb040ea70899f49261bcde5d90836e5d2' or
        $direct.recovered != true) {
      error make {msg: $"old direct Conversion diagnostic drift: ($profile)"}
    }
    {profile: $profile, cases: $item.cases, semantic: $item.semantic,
      selected: $item.selected, provenance: $provenance}
  })
  let reference = ($snapshots.0.semantic | to json --raw)
  let new_differences = ($snapshots | where {|item| ($item.semantic | to json --raw) != $reference} | get cell)
  let old_reference = ($baselines.0.semantic | to json --raw)
  let old_differences = ($baselines | where {|item| ($item.semantic | to json --raw) != $old_reference} | get profile)
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
    let new_signatures = ($new | get signature | each {|value| $value | to json --raw} | uniq)
    let old_signatures = ($old | get signature | each {|value| $value | to json --raw} | uniq)
    let expected_delta = ($id in ['bridge-conversion-error' 'direct-json-scan'])
    let old_absence = if $id == 'bridge-conversion-error' {
      $old | all {|row| ($row.passed == false and
        $row.signature.responseShape == 'array' and
        $row.signature.responseLength == 0 and
        $row.signature.responseDigest.sha256 == '4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945' and
        $row.signature.responseDigest.bytes == 2)}
    } else if $id == 'direct-json-scan' {
      $old | all {|row| ($row.passed == false and
        $row.signature.errorClass == 'Catalog Error' and
        ($row.signature.error | str contains 'read_json_auto') and
        ($row.signature.error | str contains 'json extension'))}
    } else { $old | all {|row| $row.passed == true} }
    let new_passed = ($new | all {|row| $row.passed == true})
    let within_cohorts = (($new_signatures | length) == 1 and ($old_signatures | length) == 1)
    let parity = if $expected_delta { $new_signatures.0 != $old_signatures.0 } else {
      $new_signatures.0 == $old_signatures.0
    }
    let accepted = ($within_cohorts and $old_absence and $new_passed and $parity)
    {id: $id, new: $new, old: $old, expected_delta: $expected_delta,
      classification: (if not $accepted { 'REJECTED' }
        else if $id == 'bridge-conversion-error' { 'old_wrong_empty_to_new_pass' }
        else if $id == 'direct-json-scan' { 'old_unsupported_to_new_pass' }
        else { 'parity' }),
      equal_within_cohorts: $within_cohorts, expected_old: $old_absence,
      new_passed: $new_passed, parity: $parity,
      accepted: $accepted}
  })
  let case_failures = ($case_comparisons | where accepted == false | get id)
  let case_differences = ($case_comparisons | where parity == false | get id)
  let accepted = ((($new_differences | length) == 0) and
    (($old_differences | length) == 0) and
    (($case_failures | length) == 0) and (($case_differences | length) == 0))
  let result = {schema: 'tundra-installed-json-conversion-corpus-compare/v3', admission: 'NO',
    artifact_auth: $artifact_auth,
    case_count: ($expected_ids | length), accepted: $accepted,
    new_differences: $new_differences, old_differences: $old_differences,
    case_failures: $case_failures, case_differences: $case_differences,
    case_comparisons: $case_comparisons,
    snapshots: $snapshots, baselines: $baselines}
  $result | to json --raw | save --force ($receipt_root | path join 'corpus-comparison.json')
  if not $accepted { error make {msg: 'installed JSON/Conversion corpus failed or drifted'} }
}
