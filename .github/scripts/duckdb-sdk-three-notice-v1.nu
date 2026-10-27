#!/usr/bin/env nu

# Offline, diagnostic-only source/notice association for three tiny SDK archives.
def require [condition: bool, reason: string] {
  if not $condition { error make { msg: $reason } }
}

def digest [path: string] {
  require ($path | path exists) 'EVIDENCE_MISSING'
  let entries = (ls -l $path)
  require (($entries | length) == 1 and $entries.0.type == file) 'EVIDENCE_NOT_REGULAR'
  open --raw $path | hash sha256
}

def main [ledger_path: string, producer_dir: string, installed_dir: string,
          image_dir: string, output_path: string] {
  require (not ($output_path | path exists)) 'OUTPUT_PREEXISTS'
  require ((digest $ledger_path) == $env.SDK_THREE_LEDGER_SHA256) 'LEDGER_HASH'
  let ledger = (open --raw $ledger_path | from json)
  require ($ledger.schema == 'tundra-sdk-three-selected-notice-ledger/v1' and
    $ledger.admission == 'NO' and
    $ledger.image_digest == 'sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29' and
    $ledger.official_tag_commit == 'e967e20b4727956a30592165a3c1cde5c67fa0a8' and
    $ledger.official_recipe_sha256 == 'cfbe818b5f6fc59102e55eb2ef8da94ba9273a484d799fcac5fa831ed3c2ee07' and
    $ledger.official_emscripten_license_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86' and
    $ledger.source_tar_sha256 == '61f01403aab826ce0cc37f98e2aa40a00480b80c45d2ba23146364f86d70b770' and
    $ledger.legal_tar_sha256 == '4f1e38ec664e5369e6f5cb20d16205bbcf7ccb901739eb17044d4d11c8666e70' and
    $ledger.producer.head == '22637d141f67dc92510211d7d281f6591a12a5f5' and
    $ledger.producer.run == 40434996466 and $ledger.producer.attempt == 1 and
    $ledger.producer.artifact_id == 12705817192 and
    $ledger.producer.artifact_zip_sha256 == 'b25664ca46d4f4970aa7742e3e4927a2c55b53f9466bd5644854263e0b33ab6c' and
    $ledger.producer.bill_sha256 == '93c4637dcd0cf4527814267b38a242cc626ad64f92df0a9a52928d1edc036c0f' and
    $ledger.consumer.head == 'a5b05fc1c6e51add369fd61ffe668d7a4a293bc8' and
    $ledger.consumer.source_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and
    $ledger.consumer.run == 40442338363 and $ledger.consumer.attempt == 1 and
    $ledger.sdk_archive_keys_pending_per_condition == 9 and
    $ledger.other_sdk_archive_keys_pending_per_condition == 6 and
    $ledger.generator_keys_pending_per_condition == 1 and
    $ledger.selected_graph_exhaustive == 'NO' and
    $ledger.generator_input_closure == 'UNPROVEN' and
    $ledger.glue_legal_admission == 'NO' and $ledger.legal_admission == 'NO' and $ledger.product_admission == 'NO' and
    $ledger.fleet_admission == 'NO') 'FROZEN_TUPLE_OR_SCOPE'
  require (($ledger.rows | length) == 8 and ($ledger.rows | get key | uniq | length) == 8 and
    ($ledger.consumer.artifacts | length) == 4 and
    ($ledger.consumer.artifacts | get id | uniq | length) == 4) 'LEDGER_CARDINALITY'
  let cells = ($ledger.consumer.artifacts | each {|a| $"($a.condition)/($a.profile)" } | sort)
  require ($cells == ['clean/default' 'clean/sounds' 'image/default' 'image/sounds']) 'CELL_SET'

  require ((digest ($image_dir | path join 'system_libs.py')) == $ledger.official_recipe_sha256 and
    (digest ($image_dir | path join 'LICENSE')) == $ledger.official_emscripten_license_sha256) 'PINNED_IMAGE_RECIPE_OR_LICENSE'
  require ((digest ($producer_dir | path join 'sdk-archive-recipe-source.tar.gz')) == $ledger.source_tar_sha256 and
    (digest ($producer_dir | path join 'sdk-system-legal-texts.tar.gz')) == $ledger.legal_tar_sha256) 'SOURCE_OR_LEGAL_TAR'
  require ((digest ($producer_dir | path join 'source/system/lib/libunwind/LICENSE.TXT')) == 'b5efebcaca80879234098e52d1725e6d9eb8fb96a19fce625d39184b705f7b6d' and
    (digest ($producer_dir | path join 'legal/libunwind/LICENSE.TXT')) == 'b5efebcaca80879234098e52d1725e6d9eb8fb96a19fce625d39184b705f7b6d') 'LIBUNWIND_LICENSE_SOURCE'

  let bill_path = ($producer_dir | path join 'selected-native-glue-bill-v1.json')
  require ((digest $bill_path) == $ledger.producer.bill_sha256) 'PRODUCER_BILL_HASH'
  let bill = (open --raw $bill_path | from json)
  require ($bill.schema == 'selected-native-glue-bill/v1' and
    $bill.producer_diagnostic_head == $ledger.producer.head and
    $bill.run_id == ($ledger.producer.run | into string) and
    $bill.run_attempt == ($ledger.producer.attempt | into string) and
    $bill.artifact_id == 'PENDING_UPLOAD_API_JOIN' and
    $bill.selected_graph_exhaustive == 'NO' and
    $bill.generator_input_closure == 'UNPROVEN' and
    $bill.legal_admission == 'NO' and $bill.product_admission == 'NO' and
    $bill.fleet_admission == 'NO' and
    ($bill.rows | length) == 142 and ($bill.rows | get key | uniq | length) == 142) 'PRODUCER_SCOPE'
  for condition in [image clean] {
    require ((($bill.rows | where condition == $condition and family == 'sdk') | length) == 9) 'SDK_ROW_COUNT'
  }
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
      $package.legal_admission == 'NO' and $package.product_fleet_admission == 'NO' and
      ($package.selected_notice_joins | length) == 71 and
      ($package.selected_unresolved_keys | length) == 35 and
      ($package.selected_notice_joins | get key | uniq | length) == 71) 'INSTALLED_TUPLE_OR_SCOPE'
    require ((digest ($installed_dir | path join $cell 'producer-selected-bill.json')) == $ledger.producer.bill_sha256) 'INSTALLED_PRODUCER_COPY'
    require ((digest ($installed_dir | path join $cell 'installed-notice-bytes/native-emscripten-LICENSE')) == $ledger.official_emscripten_license_sha256 and
      (digest ($installed_dir | path join $cell 'installed-notice-bytes/native-emscripten-system-lib-libunwind-LICENSE.TXT')) == 'b5efebcaca80879234098e52d1725e6d9eb8fb96a19fce625d39184b705f7b6d') 'INSTALLED_NOTICE_BYTES'
  }

  let ar_bin = if 'SDK_THREE_AR' in $env { $env.SDK_THREE_AR } else { '/emsdk/upstream/bin/llvm-ar' }
  let associations = ([image clean] | each {|condition|
    let map_path = ($producer_dir | path join $"($condition)-sdk-map-selection.txt")
    require ((digest $map_path) == ($ledger.map_sha256 | get $condition)) 'MAP_HASH'
    let map_text = (open --raw $map_path)
    let inventory_path = ($producer_dir | path join $"($condition)-sdk-archives.tsv")
    let inventory = (open --raw $inventory_path | lines | each {|line| $line | split row "\t" })
    require (($inventory | length) == 9 and ($inventory | each {|item| $item.0 } | uniq | length) == 9) 'SDK_ARCHIVE_INVENTORY'
    ['libnoexit.a' 'libstubs.a' 'libunwind-except.a'] | each {|archive|
      let rows = ($ledger.rows | where condition == $condition and archive == $archive | sort-by ordinal)
      let expected_count = if $archive == 'libstubs.a' { 2 } else { 1 }
      require (($rows | length) == $expected_count) 'ROW_COUNT'
      let first = $rows.0
      require (($rows | get archive_sha256 | uniq | length) == 1 and
        ($rows | get member_list_sha256 | uniq | length) == 1 and
        ($rows | get member | uniq | length) == $expected_count and
        ($rows | get ordinal) == (1..$expected_count | each {|n| $n })) 'PHYSICAL_MEMBER_SET'
      let inventory_match = ($inventory | where {|item| $item.0 == $archive and $item.1 == $first.archive_sha256 and $item.2 == $first.member_list_sha256 })
      require (($inventory_match | length) == 1) 'ARCHIVE_INVENTORY_JOIN'
      let archive_path = ($producer_dir | path join $"($condition)-bytes" $archive)
      require ((digest $archive_path) == $first.archive_sha256) 'ARCHIVE_BYTES'
      require ((^$ar_bin t $archive_path | hash sha256) == $first.member_list_sha256) 'AR_MEMBER_LIST_HASH'
      let member_names = (^$ar_bin t $archive_path | lines)
      require ($member_names == ($rows | get member)) 'AR_MEMBER_ORDER_OR_DUPLICATE'
      let archive_key = $"($condition)/sdk/($archive)/__archive__/1"
      let producer_rows = ($bill.rows | where key == $archive_key)
      require (($producer_rows | length) == 1 and
        $producer_rows.0.family == 'sdk' and
        $producer_rows.0.archive_sha256 == $first.archive_sha256 and
        $producer_rows.0.status == 'SELECTED_ARCHIVE_SOURCE_NOTICE_UNRESOLVED' and
        $producer_rows.0.source_path == 'UNRESOLVED' and
        $producer_rows.0.notice_sha256 == 'UNRESOLVED') 'PRODUCER_ARCHIVE_PRESTATE'
      $rows | each {|row|
        require ($row.key == $"($condition)/sdk/($archive)/($row.member)/($row.ordinal)" and
          $row.member_count == $expected_count and $row.status == 'CANDIDATE_ONLY' and
          $row.object_sha256 =~ '^[0-9a-f]{64}$' and
          $row.source_sha256 =~ '^[0-9a-f]{64}$' and
          $row.notice_sha256 =~ '^[0-9a-f]{64}$') 'ROW_FORMAT'
        let needle = ($archive + '(' + $row.member + '):')
        require (($map_text | str contains $needle) and
          (^$ar_bin p $archive_path $row.member | hash sha256) == $row.object_sha256) 'MAP_OR_OBJECT_BYTES'
        let expected_source = if $archive == 'libnoexit.a' {
          'system/lib/libc/atexit_dummy.c'
        } else if $archive == 'libunwind-except.a' {
          'system/lib/libunwind/src/Unwind-wasm.c'
        } else if $row.member == 'emscripten_libc_stubs.o' {
          'system/lib/libc/emscripten_libc_stubs.c'
        } else {
          'system/lib/libc/emscripten_syscall_stubs.c'
        }
        require ($row.source_path == $expected_source) 'SOURCE_RECIPE_PATH'
        let source_path = ($producer_dir | path join 'source' $row.source_path)
        require ((digest $source_path) == $row.source_sha256) 'SOURCE_BYTES'
        let source_text = (open --raw $source_path)
        let expected_notice = if $archive == 'libunwind-except.a' {
          'native-emscripten-system-lib-libunwind-LICENSE.TXT'
        } else {
          'native-emscripten-LICENSE'
        }
        let expected_notice_sha = if $archive == 'libunwind-except.a' {
          'b5efebcaca80879234098e52d1725e6d9eb8fb96a19fce625d39184b705f7b6d'
        } else { $ledger.official_emscripten_license_sha256 }
        let expected_license = if $archive == 'libunwind-except.a' {
          'Apache-2.0 WITH LLVM-exception'
        } else { 'MIT' }
        require ($row.notice_name == $expected_notice and
          $row.notice_sha256 == $expected_notice_sha and
          $row.selected_license == $expected_license) 'NOTICE_SELECTION'
        if $archive == 'libunwind-except.a' {
          require ($source_text | str contains 'SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception') 'LLVM_SOURCE_HEADER'
        } else {
          require (($source_text | str contains 'Emscripten is available under two separate licenses') and
            ($source_text | str contains 'MIT license')) 'EMSCRIPTEN_SOURCE_HEADER'
        }
        for profile in [default sounds] {
          let cell = $"($condition)-($profile)"
          let package = (open --raw ($installed_dir | path join $cell 'exact-package-selected-bill.json') | from json)
          let found = ($package.selected_notice_joins | where key == $archive_key)
          require (($found | length) == 1 and
            $found.0.family == 'sdk' and
            $found.0.producer_status == 'SELECTED_ARCHIVE_SOURCE_NOTICE_UNRESOLVED' and
            $found.0.installed_notice_status == 'SOURCE_OR_NOTICE_UNRESOLVED' and
            $found.0.installed_notice_sha256 == 'UNRESOLVED' and
            ($archive_key in $package.selected_unresolved_keys)) 'INSTALLED_ARCHIVE_PRESTATE'
          require ((digest ($installed_dir | path join $cell 'installed-notice-bytes' $row.notice_name)) == $row.notice_sha256) 'INSTALLED_APPLICABLE_NOTICE'
        }
        { key: $row.key, condition: $condition, archive: $archive,
          member: $row.member, ordinal: $row.ordinal,
          object_sha256: $row.object_sha256, source_sha256: $row.source_sha256,
          notice_sha256: $row.notice_sha256, selected_license: $row.selected_license,
          status: 'CANDIDATE_ONLY', installed_profiles_byte_matched: 2 }
      }
    } | flatten
  } | flatten)
  require (($associations | length) == 8 and ($associations | get key | uniq | length) == 8) 'ASSOCIATION_CARDINALITY'
  let receipt = {
    schema: 'tundra-sdk-three-selected-notice-diagnostic/v1'
    status: 'CANDIDATE_ASSOCIATIONS_ONLY'
    admission: 'NO'
    producer: $ledger.producer
    consumer: $ledger.consumer
    image_digest: $ledger.image_digest
    official_tag_commit: $ledger.official_tag_commit
    associations: $associations
    sdk_archive_keys_pending_per_condition: 9
    other_sdk_archive_keys_pending_per_condition: 6
    generator_keys_pending_per_condition: 1
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
