#!/usr/bin/env nu

# Read-only producer custody and configured-linker smoke. No EH build or archive read.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def digest [path: string] {
  require ($path | path exists) 'FILE_MISSING'
  let entries = (try { ls -lD $path } catch { [] })
  require (($entries | length) == 1 and ($entries | first).type == 'file') 'FILE_NOT_REGULAR'
  let result = (^sha256sum -- $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

def ledger_gate [ledger: record] {
  require ($ledger.schema == 'tundra-eh-linker-preflight/v1' and $ledger.scope == 'PR143_ARTIFACT_AND_PINNED_LINKER_EXECVE_SMOKE_ONLY') 'LEDGER_SCOPE'
  require ($ledger.repository == 'ryanmaclean/tundra' and $ledger.repository_id == 1164936674 and $ledger.producer_run_id == 40536996581 and $ledger.producer_attempt == 1 and $ledger.producer_head == '7c8a9cbad78e1fbcdfedb301b8e579c98dbec7d8') 'LEDGER_PRODUCER'
  require ($ledger.artifact_id == 12748008032 and $ledger.artifact_zip_size == 7075 and $ledger.artifact_zip_sha256 == '0f051bf7adec09d86d786e5642c4ef89018da6958ab04094264c52ae8bd7ab57') 'LEDGER_ARTIFACT'
  require (($ledger.members | length) == 6 and ($ledger.members | get name | uniq | length) == 6) 'LEDGER_MEMBERS'
  require ($ledger.image == 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29' and $ledger.configured_linker_path == '/emsdk/upstream/bin/wasm-ld') 'LEDGER_IMAGE'
  require ($ledger.object_input_closure == 'UNPROVEN' and $ledger.linker_archive_read == 'UNPROVEN' and $ledger.same_run_eh_outputs == 'UNPROVEN' and $ledger.sdk_archive_keys_pending_per_condition == 9 and $ledger.generator_input_closure == 'UNPROVEN' and $ledger.selected_graph_exhaustive == 'NO') 'LEDGER_ADMISSION'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] { require (($ledger | get $key) == 'NO') 'LEDGER_ADMISSION' }
}

export def authenticate [ledger_path: string, run_path: string, artifact_path: string, zip_path: string, private: string, receipt: string] {
  let ledger = (open $ledger_path)
  ledger_gate $ledger
  let run = (open $run_path)
  let artifact = (open $artifact_path)
  require ($run.id == $ledger.producer_run_id and $run.run_attempt == $ledger.producer_attempt and $run.head_sha == $ledger.producer_head and $run.event == $ledger.producer_event and $run.status == 'completed' and $run.conclusion == $ledger.producer_conclusion and $run.repository.id == $ledger.repository_id and $run.head_repository.id == $ledger.repository_id) 'PRODUCER_API_DRIFT'
  require ($artifact.id == $ledger.artifact_id and $artifact.name == $ledger.artifact_name and $artifact.size_in_bytes == $ledger.artifact_zip_size and $artifact.digest == $"sha256:($ledger.artifact_zip_sha256)" and $artifact.expired == false) 'ARTIFACT_API_DRIFT'
  require ($artifact.workflow_run.id == $ledger.producer_run_id and $artifact.workflow_run.head_sha == $ledger.producer_head and $artifact.workflow_run.repository_id == $ledger.repository_id and $artifact.workflow_run.head_repository_id == $ledger.repository_id) 'ARTIFACT_RUN_JOIN'
  require (((ls -lD $zip_path | first).size | into int) == $ledger.artifact_zip_size and (digest $zip_path) == $ledger.artifact_zip_sha256) 'ZIP_BYTES_DRIFT'
  let crc = (^unzip -tqq $zip_path | complete)
  require ($crc.exit_code == 0) 'ZIP_CRC'
  let listing = (^unzip -Z -1 $zip_path | complete)
  require ($listing.exit_code == 0) 'ZIP_LIST'
  let names = ($listing.stdout | lines)
  require (($names | length) == 6 and ($names | uniq | length) == 6 and ($names | sort) == ($ledger.members | get name | sort)) 'ZIP_MEMBER_SCOPE'
  require (not ($private | path exists)) 'PRIVATE_PREEXISTS'
  mkdir $private
  for member in $ledger.members {
    require ($member.name =~ '^[A-Za-z0-9_.-]{1,64}$' and $member.sha256 =~ '^[0-9a-f]{64}$' and $member.max_bytes <= 8192) 'LEDGER_MEMBER_SHAPE'
    let target = ($private | path join $member.name)
    ^unzip -p $zip_path $member.name o> $target
    require ($env.LAST_EXIT_CODE == 0) 'ZIP_EXTRACT'
    let bytes = ((ls -lD $target | first).size | into int)
    require ($bytes > 0 and $bytes <= $member.max_bytes and (digest $target) == $member.sha256) 'ZIP_MEMBER_BYTES'
  }
  let producer = (open ($private | path join 'controlled-object.json'))
  require ($producer.schema == $ledger.producer_receipt_schema and $producer.status == $ledger.producer_receipt_status and $producer.image_digest == $ledger.image and $producer.source_commit == $ledger.official_source_commit) 'PRODUCER_RECEIPT'
  require (($producer.conditions | length) == 2 and ($producer.conditions | get condition) == ['image' 'clean']) 'PRODUCER_CONDITIONS'
  for condition in $producer.conditions {
    let name = $condition.condition
    require ($condition.cache == $"/work/($name)/cache" and $condition.cache_initially_empty == 'YES' and $condition.build_exit == 0) 'PRODUCER_CONDITIONS'
    require ($condition.member == 'atexit_dummy.o' and $condition.ordinal == 1 and $condition.member_count == 1) 'PRODUCER_MEMBER'
    require ($condition.archive_sha256 == (digest ($private | path join $"($name)-libnoexit.a")) and $condition.object_sha256 == (digest ($private | path join $"($name)-atexit_dummy.o"))) 'PRODUCER_MEMBER_BYTES'
  }
  require ($producer.source_notice_sha256 == (digest ($private | path join 'emscripten-LICENSE')) and $producer.selected_license == 'MIT') 'PRODUCER_NOTICE'
  require ($producer.object_input_closure == 'UNPROVEN' and $producer.sdk_archive_keys_pending_per_condition == 9 and $producer.generator_input_closure == 'UNPROVEN' and $producer.selected_graph_exhaustive == 'NO') 'PRODUCER_ADMISSION'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] { require (($producer | get $key) == 'NO') 'PRODUCER_ADMISSION' }
  {schema: $ledger.schema, status: 'PR143_ARTIFACT_AUTHENTICATED_ONLY', producer_run_id: $ledger.producer_run_id, producer_attempt: $ledger.producer_attempt, producer_head: $ledger.producer_head, artifact_id: $ledger.artifact_id, artifact_zip_sha256: $ledger.artifact_zip_sha256, member_sha256: ($ledger.members | select name sha256), object_input_closure: 'UNPROVEN', linker_archive_read: 'UNPROVEN', same_run_eh_outputs: 'UNPROVEN', sdk_archive_keys_pending_per_condition: 9, generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', glue_legal_admission: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt
  true
}

export def resolve_confined_linker [path: string, root: string] {
  require (($path | path dirname) == $root and (ls -lD $root | first).type == 'dir') 'LINKER_PATH_DRIFT'
  mut current = $path
  mut seen = []
  for hop in 0..4 {
    require (not ($current in $seen)) 'LINKER_LINK_CYCLE'
    $seen = ($seen | append $current)
    let entries = (try { ls -lD $current } catch { [] })
    require (($entries | length) == 1) 'LINKER_MISSING'
    let item = ($entries | first)
    if $item.type == 'file' { return {path: $current, hops: $hop, sha256: (digest $current)} }
    require ($item.type == 'symlink' and $hop < 4) 'LINKER_LINK_SHAPE'
    let target = ($item.target | default '')
    let name = if ($target | str starts-with '/') {
      require ($target | str starts-with $"($root)/") 'LINKER_LINK_ESCAPE'
      $target | str replace $"($root)/" ''
    } else { $target }
    require ($name =~ '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$') 'LINKER_LINK_ESCAPE'
    $current = ($root | path join $name)
  }
  error make {msg: 'LINKER_LINK_DEPTH'}
}

export def probe [ledger_path: string, auth_path: string, receipt: string] {
  let ledger = (open $ledger_path)
  ledger_gate $ledger
  let auth = (open $auth_path)
  require ($auth.status == 'PR143_ARTIFACT_AUTHENTICATED_ONLY' and $auth.producer_head == $ledger.producer_head and $auth.artifact_zip_sha256 == $ledger.artifact_zip_sha256) 'AUTH_JOIN'
  let configured = (with-env {PYTHONDONTWRITEBYTECODE: '1', HOME: '/tmp'} { ^python3 -c 'import sys; sys.path.insert(0, "/emsdk/upstream/emscripten"); from tools import shared; print(shared.WASM_LD)' | complete })
  require ($configured.exit_code == 0 and ($configured.stdout | str trim) == $ledger.configured_linker_path) 'CONFIGURED_LINKER_DRIFT'
  let target = (resolve_confined_linker $ledger.configured_linker_path '/emsdk/upstream/bin')
  let elf = (^/usr/bin/readelf -h $target.path | complete)
  require ($elf.exit_code == 0 and ($elf.stdout | str contains 'ELF64') and ($elf.stdout | str contains 'Advanced Micro Devices X86-64')) 'LINKER_ELF_DRIFT'
  let smoke = (with-env {PYTHONDONTWRITEBYTECODE: '1', HOME: '/tmp'} {
    ^timeout 10s python3 -c 'import os,sys; os.execve(sys.argv[1], [sys.argv[1], "--version"], {"PATH":"/usr/bin:/bin", "HOME":"/tmp"})' $ledger.configured_linker_path | complete
  })
  require ($smoke.exit_code == 0 and ($smoke.stdout | str length) > 0 and ($smoke.stdout | str length) <= 1024 and ($smoke.stdout | str contains 'LLD')) 'DIRECT_EXECVE_SMOKE'
  {schema: $ledger.schema, status: 'PINNED_IMAGE_LINKER_PREFLIGHT_ONLY', image: $ledger.image,
   configured_linker_path: $ledger.configured_linker_path, confined_target_path: $target.path,
   confined_link_hops: $target.hops, target_elf_sha256: $target.sha256, elf_class: 'ELF64', elf_machine: 'X86_64',
   direct_execve_shape: 'PYTHON_OS_EXECVE_EXACT_PATH_VERSION_ONLY', direct_execve_exit: 0,
   actual_eh_linker_child: 'UNPROVEN', linker_archive_read: 'UNPROVEN', same_run_eh_outputs: 'UNPROVEN',
   object_input_closure: 'UNPROVEN', sdk_archive_keys_pending_per_condition: 9,
   generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', glue_legal_admission: 'NO',
   legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt
  true
}

def main [mode: string, ledger: string, first: string, second: string, third?: string, fourth?: string, fifth?: string] {
  if $mode == 'auth' {
    require ($third != null and $fourth != null and $fifth != null) 'ARGUMENTS'
    authenticate $ledger $first $second $third $fourth $fifth | ignore
  } else if $mode == 'probe' {
    probe $ledger $first $second | ignore
  } else { error make {msg: 'MODE'} }
}
