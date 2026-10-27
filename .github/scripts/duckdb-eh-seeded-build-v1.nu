#!/usr/bin/env nu

# Held one-condition build diagnostic. Hashes build products; never infers linker FD reads.
use ./duckdb-eh-seeded-archive-identity-v1.nu verify
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def info [path: string, cap: int] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') 'OUTPUT_NOT_REGULAR'
  let size = (($rows | first).size | into int)
  require ($size > 0 and $size <= $cap) 'OUTPUT_SIZE'
  let hash = (^sha256sum -- $path | complete)
  require ($hash.exit_code == 0) 'OUTPUT_HASH'
  {path: $path, bytes: $size, sha256: ($hash.stdout | split row ' ' | first)}
}

def pinned [ledger_path: string] {
  let l = (open $ledger_path)
  require ($l.schema == 'tundra-eh-seeded-build/v1' and $l.status == 'HELD_SOURCE_ONLY' and $l.condition == 'image') 'LEDGER_SCOPE'
  require ($l.source_head == '0bb7c5237dc1b047de488d67ba30b5ed05c37383' and $l.custody_head == '1d77df7776dca44731ae8267092e480a78f9f735' and $l.duckdb_wasm_head == 'f955138e5218942fb3fb097a8fdcb725222406ff' and $l.duckdb_head == '401c8061c6ece35949cac58c7770cc755710ca86' and $l.arrow_head == 'ca1c30f60d6419849dbc2a533bd0bcaff310185f') 'SOURCE_PINS'
  require ($l.expected_cache == '/work/image/cache' and $l.expected_archive == '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a' and $l.image_member == 'image-libnoexit.a' and $l.image_member_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe') 'SEED_PINS'
  require ($l.linker_archive_read == 'UNPROVEN' and $l.same_run_traced_outputs == 'UNPROVEN' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'ADMISSION_SCOPE'
  $l
}

export def map_selection [map_path: string, expected_archive: string] {
  let rows = (^grep -F 'libnoexit.a(atexit_dummy.o)' $map_path | complete)
  require ($rows.exit_code == 0 and ($rows.stdout | lines | length) == 1) 'MAP_SELECTION_COUNT'
  let fields = ($rows.stdout | str trim | split row ' ' | where $it != '')
  let expected_row_tail = ($expected_archive + '(atexit_dummy.o):(__cxa_atexit)')
  require (($fields | length) >= 2 and ($fields | last) == $expected_row_tail) 'MAP_SELECTION_ARCHIVE_PATH'
  true
}

export def finalize [ledger_path: string, prereq_ledger: string, auth_path: string, seed_receipt: string, configured_path: string, settings_path: string, workspace: string, output: string] {
  let l = (pinned $ledger_path)
  require ($configured_path == $l.expected_archive and $settings_path == $configured_path and $env.EM_CACHE == $l.expected_cache) 'SETTINGS_OR_CACHE_DRIFT'
  require ($workspace == $env.GITHUB_WORKSPACE) 'WORKSPACE_DRIFT'
  verify $prereq_ledger $auth_path $l.expected_cache $settings_path $seed_receipt | ignore
  let seed = (open $seed_receipt)
  require ($seed.status == 'HELD_SEED_IDENTITY_ONLY' and $seed.condition == 'image' and $seed.cache_absent_before_seed == 'YES' and $seed.cache_empty_before_seed == 'YES' and $seed.image_member_name == $l.image_member and $seed.seed.sha256 == $l.image_member_sha256) 'SEED_RECEIPT_JOIN'
  let build = ($workspace | path join 'duckdb-upstream/build/integrated-eh-seeded-image')
  let link_dir = ($build | path join 'CMakeFiles/duckdb_wasm.dir')
  let map = ($output | path join 'image.map')
  let objects = {
    link: (info ($link_dir | path join 'link.txt') 262144),
    objects1: (info ($link_dir | path join 'objects1.rsp') 20971520),
    linklibs: (info ($link_dir | path join 'linklibs.rsp') 20971520),
    map: (info $map 104857600),
    js: (info ($build | path join 'duckdb_wasm.js') 104857600),
    wasm: (info ($build | path join 'duckdb_wasm.wasm') 314572800)
  }
  require (($objects.map.bytes + $objects.js.bytes + $objects.wasm.bytes) <= 419430400) 'EH_OUTPUT_BUDGET'
  let command = (open --raw $objects.link.path)
  require (($command | lines | length) == 1 and ($command | str contains $"-Wl,-Map,($map)")) 'LINK_COMMAND_SHAPE'
  map_selection $map $l.expected_archive | ignore
  let manifest = (info ($workspace | path join 'custody/custody/manifest.json') 1048576)
  require ($manifest.sha256 == $l.custody_manifest_sha256) 'CUSTODY_MANIFEST_DRIFT'
  let webdb = (info ($workspace | path join 'duckdb-upstream/lib/src/webdb.cc') 1048576)
  require ($webdb.sha256 == $l.webdb_postimage_sha256) 'WEBDB_POSTIMAGE_DRIFT'
  {schema: $l.schema, status: 'SEEDED_EH_BUILD_DIAGNOSTIC_ONLY', condition: 'image', diagnostic_head: $env.GITHUB_SHA, source_heads: {source: $l.source_head, custody: $l.custody_head, duckdb_wasm: $l.duckdb_wasm_head, duckdb: $l.duckdb_head, arrow: $l.arrow_head}, producer_run_id: $l.producer_run_id, producer_attempt: $l.producer_attempt, producer_head: $l.producer_head, producer_artifact_id: $l.producer_artifact_id, producer_zip_sha256: $l.producer_zip_sha256, image_member: $l.image_member, seed: $seed.seed, target_cache_absent_before_shadow_probe: 'YES', cache_absent_before_seed: $seed.cache_absent_before_seed, cache_empty_before_seed: $seed.cache_empty_before_seed, settings_path_before_seed: $seed.settings_derived_path, settings_path_after_configure: $configured_path, settings_path_after_build: $settings_path, outputs: $objects, map_selection_class: 'ONE_EXACT_SEEDED_PATH_LIBNOEXIT_ATEXIT_DUMMY_CXA_ATEXIT_ROW', custody_manifest: $manifest.sha256, webdb_postimage: $webdb.sha256, object_input_closure: 'UNPROVEN', actual_eh_linker_child: 'UNPROVEN', linker_archive_read: 'UNPROVEN', same_run_traced_outputs: 'UNPROVEN', other_sdk_archive_keys_pending: 8, generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save ($output | path join 'build.json')
}

def main [ledger: string, prereq_ledger: string, auth: string, seed_receipt: string, configured_path: string, settings_path: string, workspace: string, output: string] {
  finalize $ledger $prereq_ledger $auth $seed_receipt $configured_path $settings_path $workspace $output
}
