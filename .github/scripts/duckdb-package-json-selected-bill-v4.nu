#!/usr/bin/env nu

# Candidate JSON bill. PR119's 38/36 bill is a pinned historical input.
# PR124's EH producer tuple is pinned; the notice graph remains unadmitted.
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
  producer_artifact_id: int,
  producer_zip_sha256: string,
  expected_wasm: string,
  expected_worker: string,
  expected_native_js: string,
  yyjson_license: string,
  notices_dir: string,
] {
  require ($condition in [image clean]) 'condition must be image or clean'
  require ($profile in [default sounds]) 'profile must be default or sounds'
  require ($producer_run == 40351355919 and $producer_artifact_id == 12666032736) 'PR124 producer run/artifact changed'
  require ($producer_zip_sha256 == '1b93d62cef81dc8d099f70d564f3654f81f0e19cda4a6b041ff806bb6f1abc11') 'PR124 producer ZIP digest changed'
  require ($producer_head == '120a368e293c56bb22032c51859d431bc3a8307d') 'PR124 producer head changed'
  require ($expected_native_js == 'fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3') 'pinned JSON native JS changed'
  let pinned_wasm = if $condition == 'image' {
    'dd806adf0140dca0256088507bc6f03aeaa48fef3c74582fec338cd61e19fbcc'
  } else { '94ffd326fc34f1859c70eeccac0eb7e0e1e074fab029735757c6075cda898117' }
  require ($expected_wasm == $pinned_wasm) 'PR124 condition EH Wasm changed'
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
    selection_evidence: 'PR124 authenticated image and clean EH maps select libjson_extension.a(yyjson.cpp.o); object-to-source attestation pending'
    license_or_role: 'MIT'
    vendored_source_sha256: '2a007a54182c6ea244ef65846a24652aa47a951a45808f1accd22752cfbb2590'
    vendored_header_sha256: '69500ac662c5f22bdeb76fba7e881135eac07fafccd9ce9ffec5b47503f38ed1'
  }
  let attributions = ($old.attributions | append $yyjson_attribution)
  let texts = ($old.texts | append {file: $yyjson_file, sha256: $yyjson_sha})
  require (($attributions | length) == ($old.attributions | length) + 1) 'attribution row count changed'
  require (($attributions | get file | uniq | length) == ($old.attributions | get file | uniq | length) + 1) 'new attribution path is not distinct'
  require (($texts | get file | uniq | length) == ($old.texts | length) + 1) 'duplicate notice path'

  let bill = {
    schema: 'tundra-duckdb-installed-json-package-selected-bill/v4'
    admission: 'NO'
    condition: $condition
    profile: $profile
    consumer_head: $consumer_head
    producer_head: $producer_head
    producer_run: $producer_run
    producer_artifact_id: $producer_artifact_id
    producer_zip_sha256: $producer_zip_sha256
    native_js_sha256: $expected_native_js
    native_wasm_sha256: $expected_wasm
    rebuilt_worker_sha256: $expected_worker
    pinned_pr119_input_bill_sha256: $old_hash
    assets: $assets
    attributions: $attributions
    texts: $texts
    excluded_on_retained_graphs: ($old.excluded_on_retained_graphs | where {|name| $name != 'yyjson'})
    yyjson_notice_source: 'PINNED_UPSTREAM_0.6.0_MIT_TEXT_WITH_VENDORED_HEADER_HASHES'
    json_selected_archive_bytes: 'UNRETAINED'
    notice_graph_complete: 'NO'
    source_texts_upstream_reverified_this_run: 'NO'
    selected_dependency_inventory_admission: 'NO'
    source_build_admission: 'NO'
    glue_legal_admission: 'NO'
    arrow_original_member_instance: 'AMBIGUOUS'
    full_EH_semantics: 'NO'
    product_fleet_admission: 'NO'
    unresolved: ($old.unresolved | append [
      'New package asset hashes do not prove complete native dependency selection'
      'Generated JavaScript glue and native source-build equivalence remain unproved'
      'PR124 selected JSON archive bytes and complete object-to-source inventory are not retained'
      'Selected yyjson.cpp.o to pinned vendored source/header requires hosted depfile or equivalent object mapping'
      'Additional JSON members or generated glue may require further selected attribution rows'
    ])
  }
  $bill | to json --indent 2 | save --force $output
}
