#!/usr/bin/env nu

def require [ok: bool, reason: string] {
  if not $ok { error make {msg: $reason} }
}

def sha [path: string] {
  require (($path | path type) == 'file') $"NOT_REGULAR:($path)"
  open --raw $path | hash sha256
}

def tsv [path: string] {
  let _ = (sha $path)
  open --raw $path | from tsv --noheaders --no-infer
}

def has_once [text: string, marker: string] {
  require (($text | split row $marker | length) == 2) $"MARKER_COUNT:($marker)"
}

def main [out: string, ledger_path: string, variant: string, diagnostic_head: string, run_id: string, run_attempt: string] {
  require ($variant == 'eh') 'EH_ONLY'
  require ($diagnostic_head =~ '^[0-9a-f]{40}$') 'DIAGNOSTIC_HEAD'
  require ($run_id =~ '^[0-9]+$') 'RUN_ID'
  require ($run_attempt =~ '^[0-9]+$') 'RUN_ATTEMPT'
  require ((sha $ledger_path) == '58f8a95afeb3d41499dd89bb37a00f8b1dbab99a78da39a5fc7e9cb56b9604ca') 'LEDGER_PIN'
  let ledger = (open --raw $ledger_path | from tsv --no-infer)
  require (($ledger | length) == 36) 'LEDGER_COUNT'
  require (($ledger | get path | uniq | length) == 36) 'LEDGER_DUPLICATE_PATH'
  let yy_notice = '45e384d3d52c73cba3a64d6e6c25d47cd738cd8a55c30629e3201046eda62947'
  require ((sha ($out | path join 'yyjson-0.6.0-LICENSE')) == $yy_notice) 'YYJSON_NOTICE'
  mut all_rows = []
  mut outputs = []
  for condition in [image clean] {
    let prefix = $"($condition)-"
    let output = {
      condition: $condition
      js_sha256: (sha ($out | path join $"($condition).js"))
      wasm_sha256: (sha ($out | path join $"($condition).wasm"))
      map_sha256: (sha ($out | path join $"($condition).map"))
    }
    $outputs = ($outputs | append $output)
    let legal_review = (open --raw ($out | path join $"($prefix)js-glue-legal-review.txt"))
    for marker in ['observed_paths=36' 'header_notice_reviewed=33' 'repo_scope_mit_reviewed=2' 'repo_scope_mit_generator_provenance_pending=1' 'superset_only_promoted=0' 'generator_input_closure=UNPROVEN' 'glue_legal_admission=NO' 'product_admission=NO'] {
      has_once $legal_review $marker
    }
    let admission = (open --raw ($out | path join 'admission.txt'))
    require (($admission | str contains 'product_admission=NO') and ($admission | str contains 'glue_legal_admission=NO')) 'ADMISSION_MARKERS'

    let json_selection_path = ($out | path join $"($prefix)json-archive-members.txt")
    let json_selection_sha = (sha $json_selection_path)
    let selected = (tsv $json_selection_path)
    require (($selected | length) == 25) 'JSON_SELECTED_COUNT'
    let member_path = ($out | path join $"($prefix)json-member-bytes.tsv")
    let members = (tsv $member_path)
    let physical = (tsv ($out | path join $"($prefix)json-physical.tsv"))
    let sources = (tsv ($out | path join $"($prefix)json-object-source.tsv"))
    let json_source_join_path = $"($prefix)json-object-source.tsv"
    let json_source_join_sha = (sha ($out | path join $json_source_join_path))
    require (($members | length) == 25 and ($physical | length) == 2 and ($sources | length) == 24) 'JSON_RECEIPT_COUNT'
    require (($selected | each {|r| $"($r.column0)/($r.column1)"} | uniq | length) == 25) 'JSON_SELECTED_DUPLICATE'
    require (($members | each {|r| $"($r.column0)/($r.column1)"} | uniq | length) == 25) 'JSON_MEMBER_DUPLICATE'
    require (($sources | get column0 | uniq | length) == 24) 'JSON_SOURCE_DUPLICATE'
    for selection in $selected {
      let archive = $selection.column0
      let member = $selection.column1
      let matching_member = ($members | where column0 == $archive | where column1 == $member)
      require (($matching_member | length) == 1) 'JSON_MEMBER_JOIN'
      let m = ($matching_member | first)
      let matching_archive = ($physical | where column0 == $archive)
      require (($matching_archive | length) == 1) 'JSON_ARCHIVE_JOIN'
      let a = ($matching_archive | first)
      require ((sha ($out | path join $a.column1)) == $a.column2) 'JSON_ARCHIVE_BYTES'
      mut source_path = 'UNRESOLVED'
      mut source_sha = 'UNRESOLVED'
      mut license = 'UNRESOLVED'
      mut notice_path = 'UNRESOLVED'
      mut notice_sha = 'UNRESOLVED'
      mut attribution_path = 'UNRESOLVED'
      mut attribution_sha = 'UNRESOLVED'
      mut status = 'UNRESOLVED_SOURCE_NOTICE'
      if $archive == 'libjson_extension.a' {
        let found = ($sources | where column0 == $member)
        require (($found | length) == 1) 'JSON_SOURCE_JOIN'
        let s = ($found | first)
        require ($s.column1 == $m.column2) 'JSON_OBJECT_SOURCE_HASH_JOIN'
        $source_path = $s.column3
        $source_sha = $s.column4
        require ((sha ($out | path join $"($prefix)json-source" $source_path)) == $source_sha) 'JSON_SOURCE_BYTES'
        $status = 'SOURCE_JOIN_NOTICE_UNRESOLVED'
        if $member == 'yyjson.cpp.o' {
          let header = (tsv ($out | path join $"($prefix)json-yyjson-header.tsv"))
          require (($header | length) == 1) 'YYJSON_HEADER_ROW'
          let h = ($header | first)
          require ($h.column0 == 'extension/json/yyjson/include/yyjson.hpp' and $h.column1 == '69500ac662c5f22bdeb76fba7e881135eac07fafccd9ce9ffec5b47503f38ed1') 'YYJSON_HEADER_PIN'
          require ((sha ($out | path join $h.column2)) == $h.column1) 'YYJSON_HEADER_BYTES'
          let source_text = (open --raw ($out | path join $"($prefix)json-source" $source_path))
          let header_text = (open --raw ($out | path join $h.column2))
          require (($source_text | str contains 'Copyright (C) 2019 Yaoyuan') and ($header_text | str contains 'Copyright (C) 2019 Yaoyuan')) 'YYJSON_ATTRIBUTION'
          $license = 'MIT'
          $notice_path = 'yyjson-0.6.0-LICENSE'
          $notice_sha = $yy_notice
          $attribution_path = $h.column2
          $attribution_sha = $h.column1
          $status = 'SELECTED_SOURCE_AND_DUAL_NOTICE_REVIEWED'
        }
      } else {
        require ($archive == 'libduckdb_web_json.a' and $member == 'json_extension.cc.o') 'JSON_UNEXPECTED_ARCHIVE_MEMBER'
      }
      $all_rows = ($all_rows | append {
        key: $"($condition)/json/($archive)/($member)/1"
        condition: $condition family: 'json' origin: 'duckdb/duckdb-wasm@f955138e5218942fb3fb097a8fdcb725222406ff'
        archive: $archive archive_sha256: $a.column2 member: $member ordinal: 1 object_sha256: $m.column2
        source_path: $source_path source_sha256: $source_sha
        witness_path: $"($prefix)json-archive-members.txt" witness_sha256: $json_selection_sha
        source_join_witness_path: $json_source_join_path source_join_witness_sha256: $json_source_join_sha
        selected_license: $license notice_path: $notice_path notice_sha256: $notice_sha
        attribution_path: $attribution_path attribution_sha256: $attribution_sha status: $status
        candidate_legal_evidence_path: 'NOT_APPLICABLE' candidate_legal_evidence_sha256: 'NOT_APPLICABLE'
      })
    }

    let gate = (open --raw ($out | path join $"($prefix)arrow-instance-gate.txt"))
    for marker in ['original_trace_js_wasm_map_byte_match=YES' 'renamed_relink_js_wasm_byte_match=YES' 'array_instance_1=EXTRACTED_RETAINED' 'compute_instance_2=NOT_EXTRACTED' 'product_admission=NO'] { has_once $gate $marker }
    let arrow_ledger_path = ($out | path join $"($prefix)arrow-instance-ledger.tsv")
    let arrow_ledger = (open --raw $arrow_ledger_path | from tsv --no-infer)
    let util = ($arrow_ledger | where member == 'util.cc.o')
    require (($util | length) == 2) 'ARROW_UTIL_INSTANCE_COUNT'
    let array = ($util | where occurrence == '1')
    let compute = ($util | where occurrence == '2')
    require (($array | length) == 1 and ($compute | length) == 1) 'ARROW_INSTANCE_OCCURRENCE'
    let selected_array = ($array | first)
    require ($selected_array.row == '20' and $selected_array.renamed_member == 'arrow-array-util-instance-1.o' and (($compute | first).renamed_member == 'arrow-compute-util-instance-2.o')) 'ARROW_INSTANCE_POSITION'
    let joins = (tsv ($out | path join $"($prefix)arrow-source" 'joins.tsv'))
    let array_join = ($joins | where column0 == 'array')
    require (($joins | length) == 2 and ($array_join | length) == 1) 'ARROW_SOURCE_JOIN'
    let j = ($array_join | first)
    require ($j.column1 == $selected_array.payload_sha256) 'ARROW_OBJECT_HASH_JOIN'
    require ((sha ($out | path join $"($prefix)arrow-source" 'array-util.cc')) == $j.column3) 'ARROW_SOURCE_BYTES'
    let arrow_txt = (open --raw ($out | path join $"($prefix)arrow.txt"))
    has_once $arrow_txt $"sha256=(sha ($out | path join $"($prefix)arrow-original.a"))"
    $all_rows = ($all_rows | append {
      key: $"($condition)/arrow/libarrow.a/util.cc.o/20:1"
      condition: $condition family: 'arrow' origin: 'apache/arrow@ca1c30f60d6419849dbc2a533bd0bcaff310185f'
      archive: 'libarrow.a' archive_sha256: (sha ($out | path join $"($prefix)arrow-original.a"))
      member: 'util.cc.o' ordinal: 20 object_sha256: $selected_array.payload_sha256
      source_path: 'cpp/src/arrow/array/util.cc' source_sha256: $j.column3
      witness_path: $"($prefix)arrow-instance-ledger.tsv" witness_sha256: (sha $arrow_ledger_path)
      source_join_witness_path: $"($prefix)arrow-source/joins.tsv" source_join_witness_sha256: (sha ($out | path join $"($prefix)arrow-source" 'joins.tsv'))
      selected_license: 'UNRESOLVED' notice_path: 'UNRESOLVED' notice_sha256: 'UNRESOLVED'
      attribution_path: 'UNRESOLVED' attribution_sha256: 'UNRESOLVED'
      candidate_legal_evidence_path: 'NOT_APPLICABLE' candidate_legal_evidence_sha256: 'NOT_APPLICABLE'
      status: 'SOURCE_JOIN_NOTICE_UNRESOLVED'
    })

    let sdk_path = ($out | path join $"($prefix)sdk-archives.tsv")
    let sdk = (tsv $sdk_path)
    let sdk_selected = (open --raw ($out | path join $"($prefix)sdk-selected.txt") | lines)
    let recipe_sha = (sha ($out | path join 'sdk-archive-recipe-source.tar.gz'))
    let candidate_legal_sha = (sha ($out | path join 'sdk-system-legal-texts.tar.gz'))
    require (($sdk | length) == 9 and ($sdk_selected | length) == 9) 'SDK_SELECTED_COUNT'
    require (($sdk | get column0 | uniq | length) == 9) 'SDK_ARCHIVE_DUPLICATE'
    for a in $sdk {
      let archive = $a.column0
      require (($sdk_selected | where {|p| $p | str ends-with $"/($archive)"} | length) == 1) 'SDK_SELECTED_PATH'
      require ((sha ($out | path join $"($condition)-bytes" $archive)) == $a.column1) 'SDK_ARCHIVE_BYTES'
      $all_rows = ($all_rows | append {
        key: $"($condition)/sdk/($archive)/__archive__/1"
        condition: $condition family: 'sdk' origin: 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29'
        archive: $archive archive_sha256: $a.column1 member: '__archive__' ordinal: 1 object_sha256: 'UNRESOLVED'
        source_path: 'UNRESOLVED' source_sha256: 'UNRESOLVED'
        witness_path: $"($prefix)sdk-archives.tsv" witness_sha256: (sha $sdk_path)
        source_join_witness_path: 'sdk-archive-recipe-source.tar.gz' source_join_witness_sha256: $recipe_sha
        selected_license: 'UNRESOLVED' notice_path: 'UNRESOLVED' notice_sha256: 'UNRESOLVED'
        attribution_path: 'UNRESOLVED' attribution_sha256: 'UNRESOLVED'
        candidate_legal_evidence_path: 'sdk-system-legal-texts.tar.gz' candidate_legal_evidence_sha256: $candidate_legal_sha
        status: 'SELECTED_ARCHIVE_SOURCE_NOTICE_UNRESOLVED'
      })
    }

    let manifest_path = ($out | path join $"($prefix)js-generator-superset.tsv")
    let manifest = (open --raw $manifest_path | from tsv --no-infer)
    require (($manifest | length) == 327) 'GLUE_MANIFEST_COUNT'
    let observed = ($manifest | where {|r| ($r.roles | str contains 'TRACE') or ($r.roles | str contains 'TRANSITIVE_INCLUDE')})
    require (($observed | length) == 36) 'GLUE_OBSERVED_COUNT'
    require (($manifest | where notice_join_status == 'TOP_LEVEL_NOTICE_CANDIDATE_UNREVIEWED' | length) == 291) 'GLUE_SUPERSET_COUNT'
    require (($observed | get path | sort) == ($ledger | get path | sort)) 'GLUE_LEDGER_SET'
    for item in $ledger {
      let match = ($observed | where path == $item.path)
      require (($match | length) == 1) 'GLUE_OBSERVED_DUPLICATE'
      let m = ($match | first)
      require ($m.sha256 == $item.sha256 and $m.notice_sha256 == $item.notice_sha256 and $m.notice_join_status == $item.association_status) 'GLUE_LEDGER_JOIN'
      require (($m.roles | str contains $item.observed_role)) 'GLUE_ROLE_JOIN'
      require (($item.association_status != 'TOP_LEVEL_NOTICE_CANDIDATE_UNREVIEWED') and ($item.selected_license == 'MIT')) 'GLUE_PROMOTION'
      let notice = if $item.notice_path == 'emscripten/LICENSE' { 'emscripten-LICENSE' } else if $item.notice_path == 'duckdb-wasm/LICENSE' { 'duckdb-wasm-LICENSE' } else { error make {msg: 'GLUE_NOTICE_PATH'} }
      require ((sha ($out | path join $"($prefix)legal-texts" $notice)) == $item.notice_sha256) 'GLUE_NOTICE_BYTES'
      $all_rows = ($all_rows | append {
        key: $"($condition)/glue/($item.path)/1"
        condition: $condition family: 'glue' origin: $item.origin
        archive: 'NOT_APPLICABLE' archive_sha256: 'NOT_APPLICABLE' member: $item.path ordinal: 1 object_sha256: 'NOT_APPLICABLE'
        source_path: $item.path source_sha256: $item.sha256
        witness_path: $"($prefix)js-generator-superset.tsv" witness_sha256: (sha $manifest_path)
        source_join_witness_path: '.github/receipts/duckdb-eh-js-glue-observed-v2.tsv' source_join_witness_sha256: (sha $ledger_path)
        selected_license: $item.selected_license notice_path: $item.notice_path notice_sha256: $item.notice_sha256
        attribution_path: 'NOT_APPLICABLE' attribution_sha256: 'NOT_APPLICABLE'
        candidate_legal_evidence_path: 'NOT_APPLICABLE' candidate_legal_evidence_sha256: 'NOT_APPLICABLE'
        status: $item.association_status
      })
    }
  }
  require (($all_rows | length) == 142) 'BILL_ROW_COUNT'
  require (($all_rows | get key | uniq | length) == 142) 'BILL_DUPLICATE_KEY'
  let bill = {
    schema: 'selected-native-glue-bill/v1'
    producer_diagnostic_head: $diagnostic_head
    pinned_source_head: '0bb7c5237dc1b047de488d67ba30b5ed05c37383'
    pinned_custody_head: '1d77df7776dca44731ae8267092e480a78f9f735'
    pinned_upstream_head: 'f955138e5218942fb3fb097a8fdcb725222406ff'
    run_id: $run_id run_attempt: $run_attempt
    artifact_name: 'duckdb-combined-custody-eh'
    artifact_id: 'PENDING_UPLOAD_API_JOIN' artifact_digest: 'PENDING_UPLOAD_API_JOIN'
    condition_outputs: $outputs
    selected_graph_exhaustive: 'NO' generator_input_closure: 'UNPROVEN'
    glue_legal_admission: 'NO' legal_admission: 'NO' product_admission: 'NO' fleet_admission: 'NO'
    rows: ($all_rows | sort-by key)
  }
  let serialized = ($bill | to json -r)
  require (($serialized | str length) < 1048576) 'BILL_SIZE_CAP'
  $serialized | save --force ($out | path join 'selected-native-glue-bill-v1.json')
}
