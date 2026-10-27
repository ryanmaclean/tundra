#!/usr/bin/env nu

# Held component: prove only the image archive seed's identity. It never runs an EH link.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def file_sha [path: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') 'NOT_REGULAR'
  let result = (^sha256sum -- $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

def identity [path: string] {
  let result = (^stat -c '%d:%i:%s:%a' -- $path | complete)
  require ($result.exit_code == 0) 'STAT_FAILED'
  {path: $path, stat: ($result.stdout | str trim), sha256: (file_sha $path)}
}

def inputs [ledger_path: string, auth_path: string, source: string, cache: string] {
  let ledger = (open $ledger_path)
  let auth = (open $auth_path)
  require ($ledger.schema == 'tundra-eh-seeded-link-prerequisites/v1' and $ledger.runnable == 'NO' and $ledger.scope == 'CONTROLLED_IMAGE_ONLY_FUTURE_EH_TRACE') 'LEDGER_SCOPE'
  require ($ledger.producer.run_id == 40536996581 and $ledger.producer.attempt == 1 and $ledger.producer.head == '7c8a9cbad78e1fbcdfedb301b8e579c98dbec7d8' and $ledger.producer.artifact_id == 12748008032 and $ledger.producer.zip_sha256 == '0f051bf7adec09d86d786e5642c4ef89018da6958ab04094264c52ae8bd7ab57') 'PRODUCER_PIN'
  require ($ledger.producer.selected_member == 'image-libnoexit.a' and $ledger.producer.selected_member_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe') 'IMAGE_SELECTION'
  require ($auth.status == 'PR143_ARTIFACT_AUTHENTICATED_ONLY' and $auth.producer_run_id == $ledger.producer.run_id and $auth.producer_attempt == $ledger.producer.attempt and $auth.producer_head == $ledger.producer.head and $auth.artifact_id == $ledger.producer.artifact_id and $auth.artifact_zip_sha256 == $ledger.producer.zip_sha256) 'AUTH_JOIN'
  require (($auth.member_sha256 | where name == 'image-libnoexit.a' | length) == 1 and ($auth.member_sha256 | where name == 'clean-libnoexit.a' | length) == 1) 'CONDITION_MEMBERS'
  require (($auth.member_sha256 | where name == 'image-libnoexit.a' | first).sha256 == $ledger.producer.selected_member_sha256) 'AUTH_IMAGE_HASH'
  require (($source | path basename) == 'image-libnoexit.a' and (file_sha $source) == $ledger.producer.selected_member_sha256) 'IMAGE_SOURCE'
  require ($cache == '/work/image/cache') 'CACHE_ROOT'
  require ($ledger.linker_archive_read == 'UNPROVEN' and $ledger.same_run_eh_outputs == 'UNPROVEN' and $ledger.legal_admission == 'NO' and $ledger.product_admission == 'NO' and $ledger.fleet_admission == 'NO') 'ADMISSION_SCOPE'
  $ledger
}

def check_dir [path: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'dir') 'DIR_MISSING_OR_SYMLINK'
}

def selected [cache: string, settings_path: string] {
  let expected = ($cache | path join 'sysroot/lib/wasm32-emscripten/libnoexit.a')
  require ($settings_path == $expected) 'SETTINGS_DERIVED_PATH_DRIFT'
  $expected
}

def inventory [cache: string, expected: string] {
  let result = (^find $cache -name libnoexit.a -print | complete)
  require ($result.exit_code == 0) 'INVENTORY_FAILED'
  let rows = ($result.stdout | lines)
  require (($rows | length) == 1 and $rows.0 == $expected) 'ALTERNATE_OR_MISSING_ARCHIVE'
}

export def seed [ledger_path: string, auth_path: string, source: string, cache: string, settings_path: string, receipt_path: string] {
  let ledger = (inputs $ledger_path $auth_path $source $cache)
  check_dir '/work'
  check_dir '/work/image'
  require (not ($cache | path exists)) 'CACHE_NOT_ABSENT'
  require ((try { ls -lD $cache } catch { [] } | length) == 0) 'CACHE_PATH_OCCUPIED'
  let expected = (selected $cache $settings_path)
  mkdir $cache
  check_dir $cache
  require ((ls -a $cache | length) == 0) 'CACHE_NOT_EMPTY'
  let sysroot = ($cache | path join 'sysroot')
  let lib = ($sysroot | path join 'lib')
  let target_dir = ($lib | path join 'wasm32-emscripten')
  for dir in [$sysroot $lib $target_dir] {
    require (not ($dir | path exists)) 'CACHE_ANCESTOR_PREEXISTS'
    mkdir $dir
    check_dir $dir
  }
  require (not ($expected | path exists)) 'TARGET_PREEXISTS'
  ^cp -- $source $expected
  require ($env.LAST_EXIT_CODE == 0) 'SEED_COPY_FAILED'
  inventory $cache $expected
  let id = (identity $expected)
  require ($id.sha256 == $ledger.producer.selected_member_sha256 and (($id.stat | split row ':').2 | into int) == 1644) 'SEED_BYTES_DRIFT'
  let resolved = (^readlink -f -- $expected | complete)
  require ($resolved.exit_code == 0 and ($resolved.stdout | str trim) == $expected) 'SEED_SYMLINK_ESCAPE'
  {schema: 'tundra-eh-seeded-archive-identity/v1', status: 'HELD_SEED_IDENTITY_ONLY', condition: 'image', producer_run_id: $ledger.producer.run_id, producer_artifact_id: $ledger.producer.artifact_id, image_member_name: $ledger.producer.selected_member, settings_derived_path: $settings_path, seed: $id, linker_archive_read: 'UNPROVEN', same_run_eh_outputs: 'UNPROVEN', object_input_closure: 'UNPROVEN', other_sdk_archive_keys_pending: 8, selected_graph_exhaustive: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt_path
}

export def verify [ledger_path: string, auth_path: string, source: string, cache: string, settings_path: string, receipt_path: string] {
  let ledger = (inputs $ledger_path $auth_path $source $cache)
  let expected = (selected $cache $settings_path)
  for dir in ['/work' '/work/image' $cache ($cache | path join 'sysroot') ($cache | path join 'sysroot/lib') ($cache | path join 'sysroot/lib/wasm32-emscripten')] { check_dir $dir }
  inventory $cache $expected
  let before = (open $receipt_path)
  require ($before.schema == 'tundra-eh-seeded-archive-identity/v1' and $before.status == 'HELD_SEED_IDENTITY_ONLY' and $before.condition == 'image' and $before.settings_derived_path == $expected) 'SEED_RECEIPT'
  let after = (identity $expected)
  require ($after == $before.seed and $after.sha256 == $ledger.producer.selected_member_sha256) 'SEED_IDENTITY_CHANGED'
  let resolved = (^readlink -f -- $expected | complete)
  require ($resolved.exit_code == 0 and ($resolved.stdout | str trim) == $expected) 'SEED_SYMLINK_ESCAPE'
  true
}

def main [mode: string, ledger: string, auth: string, source: string, cache: string, settings_path: string, receipt: string] {
  if $mode == 'seed' { seed $ledger $auth $source $cache $settings_path $receipt }
  else if $mode == 'verify' { verify $ledger $auth $source $cache $settings_path $receipt | ignore }
  else { error make {msg: 'MODE'} }
}
