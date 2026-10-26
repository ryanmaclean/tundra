#!/usr/bin/env nu

# Write the selected bill before Tauri bundles third-party-licenses/.
# The old PR119 bill is a pinned source input, not a claim about this package.
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
  expected_wasm: string,
  expected_worker: string,
] {
  require ($condition in [image clean]) 'condition must be image or clean'
  require ($profile in [default sounds]) 'profile must be default or sounds'
  require ($producer_run == 40294462862) 'producer run changed'
  require ($consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0') 'consumer head changed'
  require ($producer_head == 'fbcc2be993b9d47fd062d7833cf52f4a630649f8') 'producer head changed'
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
    } else {
      require ($digest == $old_rows.0.sha256) $"unchanged asset hash changed: ($key)"
    }
    if $name == 'duckdb-browser-eh.worker.js' {
      require ($digest == $expected_worker) 'same-run worker hash changed'
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

  let bill = {
    schema: 'tundra-duckdb-installed-package-selected-bill/v3'
    admission: 'NO'
    condition: $condition
    profile: $profile
    consumer_head: $consumer_head
    producer_head: $producer_head
    producer_run: $producer_run
    pinned_pr119_input_bill_sha256: $old_hash
    assets: $assets
    attributions: $old.attributions
    texts: $old.texts
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
    ])
  }
  $bill | to json --indent 2 | save --force $output
}
