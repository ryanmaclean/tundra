#!/usr/bin/env nu

# First successor slice only: authenticate the PR143 producer from live GitHub
# run/artifact API and ZIP bytes, then emit four NOT_BUILT package-cell slots.
# This does not authenticate an installed package or an EH linker input.
def require [ok: bool, reason: string] {
  if not $ok { error make {msg: $reason} }
}

def regular [path: string, reason: string] {
  let entries = (try { ls -lD $path } catch { [] })
  require (($entries | length) == 1 and ($entries | first).type == 'file') $reason
  $entries | first
}

def sha [path: string] {
  regular $path 'INPUT_NOT_REGULAR' | ignore
  let result = (^sha256sum $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

def member [zip: string, output: string, name: string, size: int, digest: string] {
  let target = ($output | path join $name)
  require (not ($target | path exists)) 'MEMBER_PREEXISTS'
  ^unzip -p $zip $name o> $target e> /dev/null
  require ($env.LAST_EXIT_CODE == 0) 'MEMBER_EXTRACT_FAILED'
  let entry = (regular $target 'MEMBER_NOT_REGULAR')
  require (($entry.size | into int) == $size) 'MEMBER_SIZE_DRIFT'
  require ((sha $target) == $digest) 'MEMBER_HASH_DRIFT'
  $target
}

export def validate [manifest: record, run: record, artifact: record, zip: string, output: string] {
  require ($manifest.schema == 'tundra-sdk-libnoexit-installed-preflight/v1' and $manifest.scope == 'PRODUCER_AUTH_AND_FUTURE_CELL_MANIFEST_ONLY') 'MANIFEST_SCOPE_DRIFT'
  let p = $manifest.producer
  require ($p.repository == 'ryanmaclean/tundra' and $p.run == 40536996581 and $p.attempt == 1 and $p.head == '7c8a9cbad78e1fbcdfedb301b8e579c98dbec7d8') 'PRODUCER_PIN_DRIFT'
  require ($p.artifact_id == 12748008032 and $p.artifact_name == 'duckdb-sdk-libnoexit-controlled-v2-40536996581-1' and $p.zip_bytes == 7075 and $p.zip_sha256 == '0f051bf7adec09d86d786e5642c4ef89018da6958ab04094264c52ae8bd7ab57') 'PRODUCER_PIN_DRIFT'
  require ($p.receipt_sha256 == 'f5718291d87d7ee9c6c7dd1092ab1102c13703c53b1989408f9ca6a8ca5a5b5e' and $p.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'PRODUCER_PIN_DRIFT'
  require ($p.source_commit == 'e967e20b4727956a30592165a3c1cde5c67fa0a8' and $p.source_sha256 == '7d085afbb2951dad60c4567319ae5299b669dc261767ea11f8760981c3324fb9') 'PRODUCER_PIN_DRIFT'
  require ($p.image_digest == 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29') 'PRODUCER_PIN_DRIFT'
  require ($p.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $p.object_sha256 == 'd58194aea8fda6a000d2aa479617837afc6a7d527dcc670060bd7faade9d2ea1') 'PRODUCER_PIN_DRIFT'
  require ($manifest.retired.producer_run == 40434996466 and $manifest.retired.installed_run == 40442338363) 'RETIRED_TUPLE_DRIFT'
  require (($manifest.retired.installed_artifact_ids | sort) == ([12707397953 12707094239 12708117533 12708990037] | sort)) 'RETIRED_TUPLE_DRIFT'
  require ($p.run != $manifest.retired.producer_run and $p.run != $manifest.retired.installed_run and $p.artifact_id not-in $manifest.retired.installed_artifact_ids) 'RETIRED_SUBSTITUTION'
  require ($manifest.installed_notice_path == 'third-party-licenses/ui/duckdb-runtime/native-emscripten-LICENSE') 'NOTICE_PATH_DRIFT'
  require (($manifest.future_cells | length) == 4) 'CELL_COUNT_DRIFT'
  let pairs = ($manifest.future_cells | each {|c| $"($c.condition)/($c.profile)"} | sort)
  require ($pairs == ['clean/default' 'clean/sounds' 'image/default' 'image/sounds']) 'CELL_SET_DRIFT'
  let retired_cells = {
    'image/default': {id: 12707397953, sha: '78bce8eec78e007edfc675dde994d1cd850372a23bfe769bb38b02ffb2b9f7e3'}
    'image/sounds': {id: 12707094239, sha: '2f0f7e11d2cf64a4489e78c6103988f32b4b6139f0282dfd5c80789f2f1c903d'}
    'clean/default': {id: 12708117533, sha: '9b966aee4bffa81be8975d3125182118a4c89de45d3595262254fd1ff3d514fa'}
    'clean/sounds': {id: 12708990037, sha: '4f93ad7d9f440bbb1dc53194c78186e01a6202d8d12e5a3f9b3d418396814ef8'}
  }
  for cell in $manifest.future_cells {
    require ($cell.key == $"($cell.condition)/sdk/libnoexit.a/__archive__/1" and $cell.status == 'NOT_BUILT') 'CELL_KEY_OR_STATUS_DRIFT'
    let old = ($retired_cells | get $"($cell.condition)/($cell.profile)")
    require ($cell.retired_artifact_id == $old.id and $cell.retired_zip_sha256 == $old.sha) 'RETIRED_CELL_DRIFT'
  }
  require ($manifest.package_install_verified == 'NO' and $manifest.archive_file_installed == 'NO' and $manifest.linker_input == 'UNPROVEN') 'PREMATURE_CLAIM'
  require ($manifest.object_input_closure == 'UNPROVEN' and $manifest.generator_input_closure == 'UNPROVEN' and $manifest.selected_graph_exhaustive == 'NO') 'PREMATURE_CLAIM'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] {
    require (($manifest | get $key) == 'NO') 'PREMATURE_ADMISSION'
  }

  require ($run.id == $p.run and $run.run_attempt == $p.attempt and $run.head_sha == $p.head and $run.event == 'workflow_dispatch' and $run.status == 'completed' and $run.conclusion == 'success') 'RUN_TUPLE_DRIFT'
  require ($run.repository.full_name == $p.repository) 'RUN_REPOSITORY_DRIFT'
  require ($artifact.id == $p.artifact_id and $artifact.name == $p.artifact_name and $artifact.digest == $"sha256:($p.zip_sha256)" and $artifact.size_in_bytes == $p.zip_bytes and not $artifact.expired) 'ARTIFACT_TUPLE_DRIFT'
  require ($artifact.workflow_run.id == $p.run and $artifact.workflow_run.head_sha == $p.head) 'ARTIFACT_RUN_DRIFT'
  let zip_info = (regular $zip 'ZIP_NOT_REGULAR')
  require (($zip_info.size | into int) == $p.zip_bytes and $p.zip_bytes <= 2097152) 'ZIP_SIZE_DRIFT'
  require ((sha $zip) == $p.zip_sha256) 'ZIP_HASH_DRIFT'
  let listing = (^unzip -Z1 $zip | complete)
  require ($listing.exit_code == 0) 'ZIP_LIST_FAILED'
  let expected = ['clean-atexit_dummy.o' 'clean-libnoexit.a' 'controlled-object.json' 'emscripten-LICENSE' 'image-atexit_dummy.o' 'image-libnoexit.a']
  require (($listing.stdout | lines | sort) == $expected) 'ZIP_MEMBER_SCOPE_DRIFT'
  require (not ($output | path exists)) 'OUTPUT_PREEXISTS'
  mkdir $output
  let receipt_path = (member $zip $output 'controlled-object.json' 3055 $p.receipt_sha256)
  member $zip $output 'emscripten-LICENSE' 5093 $p.notice_sha256 | ignore
  let receipt = (open $receipt_path)
  require ($receipt.schema == 'tundra-sdk-libnoexit-controlled-object/v2' and $receipt.status == 'PRODUCED_DIAGNOSTIC_ONLY') 'RECEIPT_SCOPE_DRIFT'
  require ($receipt.image_digest == $p.image_digest and $receipt.source_commit == $p.source_commit and $receipt.source_notice_sha256 == $p.notice_sha256 and $receipt.selected_license == 'MIT' and $receipt.old_producer_run == $manifest.retired.producer_run) 'RECEIPT_SOURCE_DRIFT'
  require (($receipt.conditions | length) == 2) 'CONDITION_COUNT_DRIFT'
  for condition in ['image' 'clean'] {
    let rows = ($receipt.conditions | where condition == $condition)
    require (($rows | length) == 1) 'CONDITION_COUNT_DRIFT'
    let row = ($rows | first)
    require ($row.cache == $"/work/($condition)/cache" and $row.cache_initially_empty == 'YES' and $row.build_exit == 0) 'CONDITION_BUILD_DRIFT'
    require ($row.member == 'atexit_dummy.o' and $row.ordinal == 1 and $row.member_count == 1) 'MEMBER_SCOPE_DRIFT'
    require ($row.source_sha256 == $p.source_sha256 and $row.notice_sha256 == $p.notice_sha256 and $row.archive_sha256 == $p.archive_sha256 and $row.object_sha256 == $p.object_sha256) 'CONDITION_BYTES_DRIFT'
    member $zip $output $"($condition)-libnoexit.a" 1644 $p.archive_sha256 | ignore
    member $zip $output $"($condition)-atexit_dummy.o" 1463 $p.object_sha256 | ignore
  }
  let cells = ($manifest.future_cells | each {|c| {
    condition: $c.condition, profile: $c.profile, key: $c.key, status: 'NOT_BUILT',
    installed_run: 'UNSET', installed_artifact_id: 'UNSET', package_sha256: 'UNSET',
    producer_run: $p.run, producer_artifact_id: $p.artifact_id,
    producer_zip_sha256: $p.zip_sha256, archive_sha256: $p.archive_sha256,
    object_sha256: $p.object_sha256, source_notice_sha256: $p.notice_sha256,
    retired_artifact_id: $c.retired_artifact_id, retired_zip_sha256: $c.retired_zip_sha256,
    installed_notice_path: $manifest.installed_notice_path,
    package_install_verified: 'NO', archive_file_installed: 'NO', linker_input: 'UNPROVEN'
  }})
  let result = {
    schema: 'tundra-sdk-libnoexit-installed-preflight-result/v1', status: 'PRODUCER_BYTES_AUTHENTICATED_ONLY',
    producer_run: $p.run, producer_attempt: $p.attempt, producer_head: $p.head,
    producer_artifact_id: $p.artifact_id, producer_zip_sha256: $p.zip_sha256,
    future_cells: $cells, installed_cell_status: 'NOT_BUILT',
    object_input_closure: 'UNPROVEN', generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO',
    glue_legal_admission: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'
  }
  $result | to json --raw | save ($output | path join 'preflight.json')
  $result
}

def main [output: string] {
  require (not ($output | path exists)) 'OUTPUT_PREEXISTS'
  mkdir $output
  let manifest_path = ($env.FILE_PWD | path join '..' 'receipts' 'duckdb-sdk-libnoexit-installed-preflight-v1.json')
  let manifest = (open $manifest_path)
  let p = $manifest.producer
  require ($p.repository == 'ryanmaclean/tundra' and $p.run == 40536996581 and $p.artifact_id == 12748008032) 'API_TARGET_DRIFT'
  let run_path = ($output | path join 'run-api.json')
  let artifact_path = ($output | path join 'artifact-api.json')
  let zip_path = ($output | path join 'producer.zip')
  ^gh api $"repos/($p.repository)/actions/runs/($p.run)" o> $run_path e> /dev/null
  require ($env.LAST_EXIT_CODE == 0) 'RUN_API_FAILED'
  require (((regular $run_path 'RUN_API_NOT_REGULAR').size | into int) <= 262144) 'RUN_API_OVERSIZE'
  ^gh api $"repos/($p.repository)/actions/artifacts/($p.artifact_id)" o> $artifact_path e> /dev/null
  require ($env.LAST_EXIT_CODE == 0) 'ARTIFACT_API_FAILED'
  require (((regular $artifact_path 'ARTIFACT_API_NOT_REGULAR').size | into int) <= 262144) 'ARTIFACT_API_OVERSIZE'
  ^gh api $"repos/($p.repository)/actions/artifacts/($p.artifact_id)/zip" o> $zip_path e> /dev/null
  require ($env.LAST_EXIT_CODE == 0) 'ZIP_API_FAILED'
  let extracted = ($output | path join 'authenticated')
  validate $manifest (open $run_path) (open $artifact_path) $zip_path $extracted | ignore
}
