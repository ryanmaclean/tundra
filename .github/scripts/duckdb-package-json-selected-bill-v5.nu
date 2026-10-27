#!/usr/bin/env nu

# Candidate installed bill. PR119's 38/36 bill remains a historical input.
# PR136's selected rows are a separate diagnostic, not an exhaustive legal bill.
def require [condition: bool, message: string] {
  if not $condition { error make { msg: $message } }
}

def file_hash [path: string] {
  require ($path | path exists) $"missing file: ($path)"
  let entries = (ls -l $path)
  require (($entries | length) == 1 and $entries.0.type == file) $"not one regular file: ($path)"
  open --raw $path | hash sha256
}

def main [
  pinned_bill: string,
  assets_dir: string,
  output: string,
  condition: string,
  profile: string,
  consumer_head: string,
  producer_head: string,
  producer_run: int,
  producer_attempt: int,
  producer_artifact_id: int,
  producer_artifact_digest: string,
  producer_zip_sha256: string,
  producer_bill_path: string,
  expected_wasm: string,
  expected_worker: string,
  expected_native_js: string,
  yyjson_license: string,
  notices_dir: string,
] {
  require ($condition in [image clean]) 'condition must be image or clean'
  require ($profile in [default sounds]) 'profile must be default or sounds'
  require ($producer_run == 40434996466 and $producer_attempt == 1 and $producer_artifact_id == 12705817192) 'PR136 producer run/attempt/artifact changed'
  require ($producer_artifact_digest == 'sha256:b25664ca46d4f4970aa7742e3e4927a2c55b53f9466bd5644854263e0b33ab6c') 'PR136 API digest changed'
  require ($producer_zip_sha256 == 'b25664ca46d4f4970aa7742e3e4927a2c55b53f9466bd5644854263e0b33ab6c') 'PR136 ZIP digest changed'
  require ($producer_head == '22637d141f67dc92510211d7d281f6591a12a5f5') 'PR136 producer head changed'
  require ($expected_native_js == 'fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3') 'pinned JSON native JS changed'
  let pinned_wasm = if $condition == 'image' {
    'ac2022967687136ec63c8917d77842f3c01f69291d2d024626e42a34d17953b8'
  } else { 'd267671286a6f0718a6fe1d712cd209bda84ef53f5984d0303defeb55c9f5d99' }
  require ($expected_wasm == $pinned_wasm) 'PR136 condition EH Wasm changed'
  let producer_bill_sha = (file_hash $producer_bill_path)
  require ($producer_bill_sha == '93c4637dcd0cf4527814267b38a242cc626ad64f92df0a9a52928d1edc036c0f') 'PR136 selected bill hash changed'
  let producer_bill = (open --raw $producer_bill_path | from json)
  require ($producer_bill.schema == 'selected-native-glue-bill/v1' and
    $producer_bill.producer_diagnostic_head == $producer_head and
    $producer_bill.run_id == ($producer_run | into string) and
    $producer_bill.run_attempt == ($producer_attempt | into string) and
    $producer_bill.artifact_name == 'duckdb-combined-custody-eh' and
    $producer_bill.artifact_id == 'PENDING_UPLOAD_API_JOIN' and
    $producer_bill.artifact_digest == 'PENDING_UPLOAD_API_JOIN' and
    $producer_bill.selected_graph_exhaustive == 'NO' and
    $producer_bill.generator_input_closure == 'UNPROVEN' and
    $producer_bill.glue_legal_admission == 'NO' and
    $producer_bill.legal_admission == 'NO' and
    $producer_bill.product_admission == 'NO' and
    $producer_bill.fleet_admission == 'NO') 'PR136 selected bill scope changed'
  require (($producer_bill.rows | length) == 142 and
    ($producer_bill.rows | get key | uniq | length) == 142) 'PR136 row identity changed'
  let condition_rows = ($producer_bill.rows | where condition == $condition)
  require (($condition_rows | length) == 71 and
    (($condition_rows | where family == 'json' | length) == 25) and
    (($condition_rows | where family == 'arrow' | length) == 1) and
    (($condition_rows | where family == 'sdk' | length) == 9) and
    (($condition_rows | where family == 'glue' | length) == 36)) 'PR136 selected condition changed'
  let output_rows = ($producer_bill.condition_outputs | where condition == $condition)
  require (($output_rows | length) == 1 and
    $output_rows.0.wasm_sha256 == $expected_wasm and
    $output_rows.0.js_sha256 == $expected_native_js and
    $output_rows.0.map_sha256 =~ '^[0-9a-f]{64}$') 'PR136 condition output changed'
  require ($consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0') 'consumer head changed'
  let old_hash = (file_hash $pinned_bill)
  require ($old_hash == '34381cba60c8922a159f578753b55531460792372f2d72b22769da205fe97790') 'PR119 source bill hash changed'
  let old = (open --raw $pinned_bill | decode utf-8 | from json)
  require ($old.schema == 'tundra-duckdb-selected-notice-bill/v2' and $old.admission == 'NO') 'PR119 bill schema/admission changed'
  require (($old.attributions | length) == 38 and ($old.texts | length) == 36 and ($old.assets | length) == 5) 'PR119 bill cardinality changed'

  let names = [duckdb-browser.mjs duckdb-browser-mvp.worker.js duckdb-browser-eh.worker.js duckdb-mvp.wasm duckdb-eh.wasm]
  let actual = (ls -a $assets_dir | get name | each {|p| $p | path basename } | sort)
  require ($actual == ($names | sort)) 'runtime asset path set changed'
  let assets = ($names | each {|name|
    let key = $"duckdb-runtime/($name)"
    let digest = (file_hash ($assets_dir | path join $name))
    let old_rows = ($old.assets | where path == $key)
    require (($old_rows | length) == 1) $"old asset missing/duplicate: ($key)"
    if $name == 'duckdb-eh.wasm' {
      require ($digest == $expected_wasm) 'condition EH Wasm hash changed'
      require ($digest != $old_rows.0.sha256) 'old non-JSON EH Wasm cannot form JSON bill'
    } else if $name == 'duckdb-browser-eh.worker.js' {
      require ($digest == $expected_worker) 'same-run JSON worker hash changed'
    } else {
      require ($digest == $old_rows.0.sha256) $"unchanged asset hash changed: ($key)"
    }
    { path: $key, sha256: $digest }
  } | sort-by path)
  require (($assets | get path | uniq | length) == 5) 'duplicate selected runtime asset'
  let paths = ($old.texts | get file)
  require (($paths | uniq | length) == 36) 'duplicate notice text path'
  for attribution in $old.attributions {
    let matches = ($old.texts | where file == $attribution.file)
    require (($matches | length) == 1 and $matches.0.sha256 == $attribution.source_sha256) 'attribution/text join changed'
  }
  require ('yyjson' in $old.excluded_on_retained_graphs) 'historical yyjson exclusion missing'
  let yyjson_sha = (file_hash $yyjson_license)
  require ($yyjson_sha == '45e384d3d52c73cba3a64d6e6c25d47cd738cd8a55c30629e3201046eda62947') 'yyjson full MIT text changed'
  let yyjson_file = 'third-party-licenses/ui/duckdb-runtime/native-yyjson-0.6.0-LICENSE'
  let yyjson_target = ($notices_dir | path join 'native-yyjson-0.6.0-LICENSE')
  require (not ($yyjson_target | path exists)) 'yyjson notice target already exists'
  require (($old.texts | where file == $yyjson_file | length) == 0) 'yyjson text already in PR119 bill'
  require (($old.attributions | where component == 'yyjson' | length) == 0) 'yyjson attribution already in PR119 bill'
  cp $yyjson_license $yyjson_target
  require ((file_hash $yyjson_target) == $yyjson_sha) 'copied yyjson text mismatch'
  let yyjson_attribution = {
    surface: 'native'
    component: 'yyjson'
    source_url: 'https://raw.githubusercontent.com/ibireme/yyjson/7b210d66cb73536f351e8a82a63508bc77bffd20/LICENSE'
    source_sha256: $yyjson_sha
    file: $yyjson_file
    selection_evidence: 'PR136 selected bill binds the yyjson object, retained source, MIT notice, and modified header for this condition; full graph admission remains NO'
    license_or_role: 'MIT'
    vendored_source_sha256: '2a007a54182c6ea244ef65846a24652aa47a951a45808f1accd22752cfbb2590'
    vendored_header_sha256: '69500ac662c5f22bdeb76fba7e881135eac07fafccd9ce9ffec5b47503f38ed1'
  }
  let attributions = ($old.attributions | append $yyjson_attribution)
  let texts = ($old.texts | append {file: $yyjson_file, sha256: $yyjson_sha})
  require (($attributions | length) == ($old.attributions | length) + 1) 'attribution row count changed'
  require (($attributions | get file | uniq | length) == ($old.attributions | get file | uniq | length) + 1) 'new attribution path is not distinct'
  require (($texts | get file | uniq | length) == ($old.texts | length) + 1) 'duplicate notice path'

  let selected_notice_joins = ($condition_rows | each {|row|
    require ($row.status != 'TOP_LEVEL_NOTICE_CANDIDATE_UNREVIEWED') 'superset-only glue promoted'
    let notice = $row.notice_path
    let installed_name = if $notice == 'yyjson-0.6.0-LICENSE' {
      'native-yyjson-0.6.0-LICENSE'
    } else if $notice == 'emscripten/LICENSE' {
      'native-emscripten-LICENSE'
    } else if $notice == 'duckdb-wasm/LICENSE' {
      'native-duckdb-wasm.txt'
    } else {
      require ($notice == 'UNRESOLVED' and $row.notice_sha256 == 'UNRESOLVED') 'unrecognized selected notice'
      'UNRESOLVED'
    }
    let installed_path = if $installed_name == 'UNRESOLVED' { 'UNRESOLVED' } else {
      $"third-party-licenses/ui/duckdb-runtime/($installed_name)"
    }
    if $installed_name != 'UNRESOLVED' {
      require ((file_hash ($notices_dir | path join $installed_name)) == $row.notice_sha256) 'selected notice source byte mismatch'
      let text = ($texts | where file == $installed_path)
      require (($text | length) == 1 and $text.0.sha256 == $row.notice_sha256) 'selected notice bill join mismatch'
    }
    let status = if $installed_name == 'UNRESOLVED' { 'SOURCE_OR_NOTICE_UNRESOLVED' } else if $row.status == 'REPO_SCOPE_MIT_GENERATOR_PROVENANCE_PENDING' { 'NOTICE_BYTES_MATCH_GENERATOR_PROVENANCE_PENDING' } else { 'NOTICE_BYTES_MATCH' }
    {key: $row.key, family: $row.family, producer_status: $row.status,
      source_notice_path: $notice, source_notice_sha256: $row.notice_sha256,
      installed_notice_path: $installed_path, installed_notice_sha256: $row.notice_sha256,
      installed_notice_status: $status,
      attribution_sha256: $row.attribution_sha256,
      attribution_install_status: (if $row.attribution_sha256 == 'UNRESOLVED' { 'UNRESOLVED' } else if $row.attribution_sha256 == 'NOT_APPLICABLE' { 'NOT_APPLICABLE' } else { 'HEADER_BYTES_RETAINED_NOT_INSTALLED' })}
  })
  require (($selected_notice_joins | length) == 71) 'selected notice join count'
  let selected_unresolved = ($selected_notice_joins | where installed_notice_status != 'NOTICE_BYTES_MATCH' | get key)

  let bill = {
    schema: 'tundra-duckdb-installed-json-package-selected-bill/v5'
    admission: 'NO'
    condition: $condition
    profile: $profile
    consumer_head: $consumer_head
    producer_head: $producer_head
    producer_run: $producer_run
    producer_attempt: $producer_attempt
    producer_artifact_id: $producer_artifact_id
    producer_artifact_digest: $producer_artifact_digest
    producer_zip_sha256: $producer_zip_sha256
    producer_bill_sha256: $producer_bill_sha
    condition_output: $output_rows.0
    native_js_sha256: $expected_native_js
    native_wasm_sha256: $expected_wasm
    rebuilt_worker_sha256: $expected_worker
    pinned_pr119_input_bill_sha256: $old_hash
    assets: $assets
    attributions: $attributions
    texts: $texts
    historical_notice_rows: {attributions: 39, texts: 37, graph_closure: 'NO'}
    selected_notice_joins: $selected_notice_joins
    selected_unresolved_keys: $selected_unresolved
    excluded_on_retained_graphs: ($old.excluded_on_retained_graphs | where {|name| $name != 'yyjson'})
    yyjson_notice_source: 'PINNED_UPSTREAM_0.6.0_MIT_TEXT_WITH_VENDORED_HEADER_HASHES'
    json_selected_archive_bytes: 'RETAINED_BOUNDED'
    notice_graph_complete: 'NO'
    source_texts_upstream_reverified_this_run: 'NO'
    selected_dependency_inventory_admission: 'NO'
    source_build_admission: 'NO'
    glue_legal_admission: 'NO'
    arrow_original_member_instance: 'ARRAY_1_SELECTED_COMPUTE_2_UNSELECTED_EH_DIAGNOSTIC'
    selected_graph_exhaustive: 'NO'
    generator_input_closure: 'UNPROVEN'
    legal_admission: 'NO'
    full_EH_semantics: 'NO'
    product_fleet_admission: 'NO'
    unresolved: ($old.unresolved | append [
      'New package asset hashes do not prove complete native dependency selection'
      'Generated JavaScript glue and native source-build equivalence remain unproved'
      'PR136 bill retains selected JSON archive/object/source bytes but does not close every selected notice'
      'PR136 selected Arrow array object notice and SDK archive source/notice scopes remain unresolved'
      'The modified yyjson header attribution is retained as evidence, not installed as a separate notice file'
      'The library_sigs.js generator provenance and full generator input closure remain unproved'
    ])
  }
  $bill | to json --indent 2 | save --force $output
}
