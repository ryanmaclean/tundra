#!/usr/bin/env nu

# Offline, diagnostic-only association of 25 selected non-SDK keys per EH condition.
def require [condition: bool, reason: string] {
  if not $condition { error make { msg: $reason } }
}

def digest [path: string] {
  require ($path | path exists) 'EVIDENCE_MISSING'
  let entries = (ls -l $path)
  require (($entries | length) == 1 and $entries.0.type == file) 'EVIDENCE_NOT_REGULAR'
  open --raw $path | hash sha256
}

def primary [root: string, repo: string, commit: string, relative: string] {
  require ($repo in ['duckdb/duckdb' 'duckdb/duckdb-wasm' 'apache/arrow']) 'PRIMARY_REPO'
  require ($commit =~ '^[0-9a-f]{40}$' and $relative !~ '(^/|\.\.|\\)') 'PRIMARY_PATH'
  $root | path join $repo $commit $relative
}

def main [ledger_path: string, producer_dir: string, installed_dir: string,
          primary_dir: string, output_path: string] {
  require (not ($output_path | path exists)) 'OUTPUT_PREEXISTS'
  require ((digest $ledger_path) == $env.NOTICE_LEDGER_SHA256) 'LEDGER_HASH'
  let ledger = (open --raw $ledger_path | from json)
  require ($ledger.schema == 'tundra-selected-notice-25-ledger/v1' and
    $ledger.admission == 'NO' and $ledger.producer.head == '22637d141f67dc92510211d7d281f6591a12a5f5' and
    $ledger.producer.run == 40434996466 and $ledger.producer.attempt == 1 and
    $ledger.producer.artifact_id == 12705817192 and
    $ledger.producer.artifact_zip_sha256 == 'b25664ca46d4f4970aa7742e3e4927a2c55b53f9466bd5644854263e0b33ab6c' and
    $ledger.producer.bill_sha256 == '93c4637dcd0cf4527814267b38a242cc626ad64f92df0a9a52928d1edc036c0f' and
    $ledger.consumer.head == 'a5b05fc1c6e51add369fd61ffe668d7a4a293bc8' and
    $ledger.consumer.run == 40442338363 and $ledger.consumer.attempt == 1 and
    $ledger.consumer.source_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and
    $ledger.sdk_keys_pending == 9 and $ledger.generator_keys_pending == 1 and
    $ledger.selected_graph_exhaustive == 'NO' and
    $ledger.generator_input_closure == 'UNPROVEN' and
    $ledger.glue_legal_admission == 'NO' and $ledger.legal_admission == 'NO' and
    $ledger.product_admission == 'NO' and $ledger.fleet_admission == 'NO') 'FROZEN_TUPLE_OR_SCOPE'
  require (($ledger.rows | length) == 25 and
    ($ledger.rows | get suffix | uniq | length) == 25 and
    ($ledger.consumer.artifacts | length) == 4 and
    ($ledger.consumer.artifacts | get id | uniq | length) == 4) 'LEDGER_CARDINALITY'
  let bill_path = ($producer_dir | path join 'selected-native-glue-bill-v1.json')
  require ((digest $bill_path) == $ledger.producer.bill_sha256) 'PRODUCER_BILL_HASH'
  let bill = (open --raw $bill_path | from json)
  require ($bill.schema == 'selected-native-glue-bill/v1' and
    $bill.producer_diagnostic_head == $ledger.producer.head and
    $bill.run_id == ($ledger.producer.run | into string) and
    $bill.run_attempt == ($ledger.producer.attempt | into string) and
    $bill.artifact_id == 'PENDING_UPLOAD_API_JOIN' and
    $bill.artifact_digest == 'PENDING_UPLOAD_API_JOIN' and
    $bill.selected_graph_exhaustive == 'NO' and
    $bill.generator_input_closure == 'UNPROVEN' and
    $bill.glue_legal_admission == 'NO' and $bill.legal_admission == 'NO' and
    $bill.product_admission == 'NO' and $bill.fleet_admission == 'NO') 'PRODUCER_SCOPE'
  require (($bill.rows | length) == 142 and ($bill.rows | get key | uniq | length) == 142) 'PRODUCER_KEY_SET'
  for condition in [image clean] {
    let part = ($bill.rows | where condition == $condition)
    require (($part | length) == 71 and
      (($part | where family == 'json' | length) == 25) and
      (($part | where family == 'arrow' | length) == 1) and
      (($part | where family == 'sdk' | length) == 9) and
      (($part | where family == 'glue' | length) == 36) and
      (($part | where status == 'SUPERSET_ONLY' | length) == 0)) 'SELECTED_KEY_SHAPE'
  }
  let artifact_keys = ($ledger.consumer.artifacts | each {|a| $"($a.condition)/($a.profile)" } | sort)
  require ($artifact_keys == ['clean/default' 'clean/sounds' 'image/default' 'image/sounds']) 'CONSUMER_CELL_SET'
  for artifact in $ledger.consumer.artifacts {
    let cell = $"($artifact.condition)-($artifact.profile)"
    let package = (open --raw ($installed_dir | path join $cell 'exact-package-selected-bill.json') | from json)
    require ($package.schema == 'tundra-duckdb-exact-eh-package-selected-bill/v2' and
      $package.consumer_head == $ledger.consumer.source_head and
      $package.producer_head == $ledger.producer.head and
      $package.producer_run == $ledger.producer.run and
      $package.producer_attempt == $ledger.producer.attempt and
      $package.producer_artifact_id == $ledger.producer.artifact_id and
      $package.producer_artifact_digest == $"sha256:($ledger.producer.artifact_zip_sha256)" and
      $package.producer_bill_sha256 == $ledger.producer.bill_sha256 and
      $package.condition == $artifact.condition and $package.profile == $artifact.profile and
      $package.selected_graph_exhaustive == 'NO' and
      $package.generator_input_closure == 'UNPROVEN' and
      $package.legal_admission == 'NO' and
      $package.product_fleet_admission == 'NO' and
      ($package.selected_notice_joins | length) == 71 and
      ($package.selected_unresolved_keys | length) == 35) 'INSTALLED_TUPLE_OR_SCOPE'
    let producer_copy = ($installed_dir | path join $cell 'producer-selected-bill.json')
    require ((digest $producer_copy) == $ledger.producer.bill_sha256) 'INSTALLED_PRODUCER_COPY'
    require (($package.selected_notice_joins | get key | uniq | length) == 71 and
      ($package.selected_unresolved_keys | uniq | length) == 35) 'INSTALLED_KEY_DUPLICATE'
  }
  let associations = ($ledger.rows | each {|entry|
    require ($entry.suffix =~ '^(json|arrow)/' and
      $entry.family in [json arrow] and
      $entry.archive_sha256 =~ '^[0-9a-f]{64}$' and
      $entry.object_sha256 =~ '^[0-9a-f]{64}$' and
      $entry.source_sha256 =~ '^[0-9a-f]{64}$' and
      $entry.notice_sha256 =~ '^[0-9a-f]{64}$') 'ROW_FORMAT'
    let src = (primary $primary_dir $entry.primary_repo $entry.primary_commit $entry.source_path)
    let notice = (primary $primary_dir $entry.primary_repo $entry.primary_commit $entry.notice_path)
    require ((digest $src) == $entry.source_sha256 and
      (digest $notice) == $entry.notice_sha256) 'PRIMARY_SOURCE_OR_NOTICE_BYTES'
    if $entry.attribution_path != 'NOT_APPLICABLE' {
      let attribution = (primary $primary_dir $entry.primary_repo $entry.primary_commit $entry.attribution_path)
      require ((digest $attribution) == $entry.attribution_sha256) 'PRIMARY_ATTRIBUTION_BYTES'
    }
    if $entry.suffix == 'json/libduckdb_web_json.a/json_extension.cc.o/1' {
      let cmake = (primary $primary_dir $entry.primary_repo $entry.primary_commit $entry.cmake_path)
      require ((digest $cmake) == $entry.cmake_sha256 and
        ('duckdb_web_json' in (open --raw $cmake)) and
        ('src/extensions/json_extension.cc' in (open --raw $cmake))) 'WEB_JSON_CMAKE_SOURCE'
    }
    let per_condition = ([image clean] | each {|condition|
      let key = $"($condition)/($entry.suffix)"
      let found = ($bill.rows | where key == $key)
      require (($found | length) == 1) 'SELECTED_ROW_MISSING_OR_DUPLICATE'
      let row = $found.0
      require ($row.family == $entry.family and $row.archive == $entry.archive and
        $row.archive_sha256 == $entry.archive_sha256 and
        $row.member == $entry.member and $row.ordinal == $entry.ordinal and
        $row.object_sha256 == $entry.object_sha256 and
        $row.status in ['SOURCE_JOIN_NOTICE_UNRESOLVED' 'UNRESOLVED_SOURCE_NOTICE'] and
        $row.notice_sha256 == 'UNRESOLVED' and
        $row.attribution_sha256 == 'UNRESOLVED') 'SELECTED_ROW_DRIFT'
      for field in ['witness' 'source_join_witness'] {
        let rel = ($row | get $"($field)_path")
        let expected_sha = ($row | get $"($field)_sha256")
        require ($rel !~ '(^/|\.\.|\\)' and
          (digest ($producer_dir | path join $rel)) == $expected_sha) 'SOURCE_WITNESS_BYTES'
      }
      if $entry.suffix == 'json/libduckdb_web_json.a/json_extension.cc.o/1' {
        require ($row.source_path == 'UNRESOLVED' and $row.source_sha256 == 'UNRESOLVED') 'WEB_JSON_PRODUCER_PENDING'
        let commands = (open --raw ($producer_dir | path join $"($condition)-compile_commands.json") | from json)
        let matches = ($commands | where {|command| ($command.file | str ends-with '/lib/src/extensions/json_extension.cc') and ($command.command | str contains 'CMakeFiles/duckdb_web_json.dir/src/extensions/json_extension.cc.o')})
        require (($matches | length) == 1) 'WEB_JSON_COMPILE_COMMAND'
        let log = (open --raw ($producer_dir | path join $"($condition)-build.log"))
        require ($log | str contains 'Building CXX object CMakeFiles/duckdb_web_json.dir/src/extensions/json_extension.cc.o') 'WEB_JSON_BUILD_LOG'
        let archive = ($producer_dir | path join $"($condition)-json-bytes" $entry.archive)
        require ((digest $archive) == $entry.archive_sha256) 'WEB_JSON_ARCHIVE_BYTES'
        let ar_bin = if 'NOTICE_AR' in $env { $env.NOTICE_AR } else { 'ar' }
        let member_sha = (^$ar_bin p $archive $entry.member | hash sha256)
        require ($member_sha == $entry.object_sha256) 'WEB_JSON_OBJECT_BYTES'
      } else {
        require ($row.source_path == $entry.source_path and
          $row.source_sha256 == $entry.source_sha256) 'SELECTED_SOURCE_JOIN'
        let retained_source = if $entry.family == 'arrow' {
          $producer_dir | path join $"($condition)-arrow-source" 'array-util.cc'
        } else {
          $producer_dir | path join $"($condition)-json-source" $entry.source_path
        }
        require ((digest $retained_source) == $entry.source_sha256) 'RETAINED_SOURCE_BYTES'
        if $entry.family == 'arrow' {
          let ledger_rows = (open --raw ($producer_dir | path join $"($condition)-arrow-instance-ledger.tsv") | from tsv)
          let selected = ($ledger_rows | where {|item| ($item.row | into int) == $entry.ordinal and $item.member == $entry.member and $item.payload_sha256 == $entry.object_sha256})
          require (($selected | length) == 1) 'ARROW_ORDINAL_OBJECT'
          let joins = (open --raw ($producer_dir | path join $"($condition)-arrow-source/joins.tsv") | lines | each {|line| $line | split row "\t" })
          let matching = ($joins | where {|item| ($item.0 == 'array') and ($item.1 == $entry.object_sha256) and ($item.3 == $entry.source_sha256)})
          require (($matching | length) == 1) 'ARROW_SOURCE_JOIN'
        } else {
          let joins = (open --raw ($producer_dir | path join $"($condition)-json-object-source.tsv") | lines | each {|line| $line | split row "\t" })
          let matching = ($joins | where {|item| ($item.0 == $entry.member) and ($item.1 == $entry.object_sha256) and ($item.3 == $entry.source_path) and ($item.4 == $entry.source_sha256)})
          require (($matching | length) == 1) 'JSON_OBJECT_SOURCE_JOIN'
        }
      }
      let cells = ([default sounds] | each {|profile|
        let cell = $"($condition)-($profile)"
        let package = (open --raw ($installed_dir | path join $cell 'exact-package-selected-bill.json') | from json)
        let installed_row = ($package.selected_notice_joins | where key == $key)
        require (($installed_row | length) == 1 and
          $installed_row.0.installed_notice_status == 'SOURCE_OR_NOTICE_UNRESOLVED' and
          ($key in $package.selected_unresolved_keys)) 'INSTALLED_ROW_PRESTATE'
        require ((digest ($installed_dir | path join $cell 'installed-notice-bytes' $entry.installed_notice_name)) ==
          $entry.notice_sha256) 'INSTALLED_NOTICE_BYTES'
        if $entry.attribution_path != 'NOT_APPLICABLE' {
          require ((digest ($installed_dir | path join $cell 'installed-notice-bytes' $entry.installed_attribution_name)) ==
            $entry.attribution_sha256) 'INSTALLED_ATTRIBUTION_BYTES'
        }
        $profile
      })
      require (($cells | length) == 2) 'INSTALLED_PROFILE_SET'
      { key: $key, status: $entry.candidate_status, source_sha256: $entry.source_sha256,
        object_sha256: $entry.object_sha256, notice_sha256: $entry.notice_sha256,
        attribution_sha256: $entry.attribution_sha256, installed_profiles_byte_matched: 2 }
    })
    $per_condition
  } | flatten | sort-by key)
  require (($associations | length) == 50 and
    ($associations | get key | uniq | length) == 50) 'ASSOCIATION_CARDINALITY'
  let receipt = {
    schema: 'tundra-selected-notice-25-diagnostic/v1'
    admission: 'NO'
    status: 'CANDIDATE_ASSOCIATIONS_ONLY'
    producer: $ledger.producer
    consumer: $ledger.consumer
    associations: $associations
    pending_sdk_keys_per_condition: 9
    pending_generator_keys_per_condition: 1
    selected_graph_exhaustive: 'NO'
    generator_input_closure: 'UNPROVEN'
    glue_legal_admission: 'NO'
    legal_admission: 'NO'
    product_admission: 'NO'
    fleet_admission: 'NO'
  }
  let encoded = ($receipt | to json --raw)
  require (($encoded | str length) < 1048576) 'RECEIPT_CAP'
  $encoded | save -f $output_path
}
