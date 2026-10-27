#!/usr/bin/env nu

# Four package-resource cells. The archive is retained as a diagnostic resource;
# this script never claims that the EH linker read it.
def require [ok: bool, reason: string] {
  if not $ok { error make {msg: $reason} }
}

def regular [path: string, reason: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') $reason
  $rows | first
}

def digest [path: string] {
  regular $path 'FILE_NOT_REGULAR' | ignore
  let result = (^sha256sum $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

def fixed [ledger: record, preflight: record, condition: string, profile: string] {
  require ($ledger.schema == 'tundra-sdk-libnoexit-installed-cells/v1' and $ledger.scope == 'FOUR_NEW_PACKAGE_NOTICE_AND_ARCHIVE_EVIDENCE_CELLS_ONLY') 'LEDGER_SCOPE_DRIFT'
  require ($ledger.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0') 'CONSUMER_HEAD_DRIFT'
  require ($ledger.old_ui.run == 40275385842 and $ledger.old_ui.artifact_id == 12630875549 and $ledger.old_ui.artifact_name == 'leptos-ui-dist' and $ledger.old_ui.zip_sha256 == 'd0f2845347a404c37265b008626bbce4d3b3e97033f3b98383389400b4fe3685') 'OLD_UI_PIN_DRIFT'
  let old_assets = {
    'duckdb-browser.mjs': '8ccf3fe3c6bd87d5c3e397f01d40cc11ea6373b016a9cf5e954d39ee22c3aa49'
    'duckdb-browser-eh.worker.js': 'feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4'
    'duckdb-eh.wasm': 'd71b7e4575c98b87e09ac799c574d770aa5ee4df5e6c6cf36b6e277d92b8d2c2'
    'duckdb-browser-mvp.worker.js': 'd7920b73954d99ffa59175db35fe24e7936f8a003144386686b53c853c46eaef'
    'duckdb-mvp.wasm': '7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791'
  }
  require ($ledger.old_ui.assets == $old_assets) 'OLD_UI_ASSET_DRIFT'
  require ($ledger.producer_run == 40536996581 and $ledger.producer_artifact_id == 12748008032 and $ledger.producer_zip_sha256 == '0f051bf7adec09d86d786e5642c4ef89018da6958ab04094264c52ae8bd7ab57') 'PRODUCER_TUPLE_DRIFT'
  require ($ledger.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $ledger.object_sha256 == 'd58194aea8fda6a000d2aa479617837afc6a7d527dcc670060bd7faade9d2ea1' and $ledger.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'PRODUCER_BYTES_DRIFT'
  require ($ledger.notice_resource == 'third-party-licenses/ui/duckdb-runtime/native-emscripten-LICENSE' and $ledger.archive_resource == 'third-party-licenses/ui/duckdb-runtime/native-emscripten-libnoexit.a' and $ledger.cell_resource == 'third-party-licenses/ui/duckdb-runtime/native-emscripten-libnoexit-cell.json') 'RESOURCE_PATH_DRIFT'
  require (($ledger.cells | sort) == ['clean/default' 'clean/sounds' 'image/default' 'image/sounds'] and ($ledger.cells | length) == 4) 'CELL_SET_DRIFT'
  let old_packages = [
    '4a59cb27421939086c8af5594769292e8b8c8ca3196cd7efac8782e7e0fdd28f'
    '12b6bac4d55381456a32ef9bf13aa18d9231bcaabbad8bcb6c0214868f2090db'
    '1166cacee360c1d7219d8a8bc2b083a071826c55f57bed779e280bd9b48300a1'
    '3298bc18016485f8f44218b134cd5285f695a090ca0b87d20c01b1a86147a7ea'
  ]
  require ($ledger.retired_package_sha256 == $old_packages) 'RETIRED_PACKAGE_PIN_DRIFT'
  require ($"($condition)/($profile)" in $ledger.cells) 'CELL_IDENTITY_DRIFT'
  require ($ledger.max_deb_bytes == 201326592 and $ledger.max_receipt_bytes == 2097152) 'CAP_DRIFT'
  require ($ledger.linker_input == 'UNPROVEN' and $ledger.selected_graph_exhaustive == 'NO' and $ledger.generator_input_closure == 'UNPROVEN') 'PREMATURE_CLOSURE'
  for key in ['legal_admission' 'product_admission' 'fleet_admission'] {
    require (($ledger | get $key) == 'NO') 'PREMATURE_ADMISSION'
  }
  require ($preflight.schema == $ledger.preflight_schema and $preflight.status == $ledger.preflight_status and $preflight.installed_cell_status == 'NOT_BUILT') 'PREFLIGHT_STATUS_DRIFT'
  require ($preflight.producer_run == $ledger.producer_run and $preflight.producer_artifact_id == $ledger.producer_artifact_id and $preflight.producer_zip_sha256 == $ledger.producer_zip_sha256) 'PREFLIGHT_PRODUCER_DRIFT'
  require (($preflight.future_cells | length) == 4) 'PREFLIGHT_CELL_DRIFT'
  let cells = ($preflight.future_cells | each {|c| $"($c.condition)/($c.profile)"} | sort)
  require ($cells == ($ledger.cells | sort)) 'PREFLIGHT_CELL_DRIFT'
  let selected = ($preflight.future_cells | where condition == $condition and profile == $profile)
  require (($selected | length) == 1) 'PREFLIGHT_CELL_DRIFT'
  let cell = ($selected | first)
  require ($cell.status == 'NOT_BUILT' and $cell.key == $"($condition)/sdk/libnoexit.a/__archive__/1") 'PREFLIGHT_CELL_DRIFT'
  require ($cell.archive_sha256 == $ledger.archive_sha256 and $cell.object_sha256 == $ledger.object_sha256 and $cell.source_notice_sha256 == $ledger.notice_sha256) 'PREFLIGHT_BYTES_DRIFT'
  require ($cell.package_install_verified == 'NO' and $cell.archive_file_installed == 'NO' and $cell.linker_input == 'UNPROVEN') 'PREFLIGHT_CLAIM_DRIFT'
  require ($preflight.object_input_closure == 'UNPROVEN' and $preflight.generator_input_closure == 'UNPROVEN' and $preflight.selected_graph_exhaustive == 'NO') 'PREFLIGHT_CLAIM_DRIFT'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] {
    require (($preflight | get $key) == 'NO') 'PREFLIGHT_CLAIM_DRIFT'
  }
}

def source_root [root: string] {
  let info = (try { ls -lD $root } catch { [] })
  require (($info | length) == 1 and ($info | first).type == 'dir') 'SOURCE_ROOT_DRIFT'
}

export def prepare [ledger: record, preflight: record, authenticated: string, old_zip: string, consumer: string, condition: string, profile: string] {
  fixed $ledger $preflight $condition $profile
  source_root $consumer
  require ((digest $old_zip) == $ledger.old_ui.zip_sha256) 'OLD_UI_ZIP_DRIFT'
  require (((regular $old_zip 'OLD_UI_ZIP_NOT_REGULAR').size | into int) <= 67108864) 'OLD_UI_ZIP_CAP'
  let listing = (^unzip -Z1 $old_zip | complete)
  require ($listing.exit_code == 0) 'OLD_UI_ZIP_LIST_FAILED'
  let names = ($listing.stdout | lines)
  require (($names | length) == (($names | uniq) | length) and ($names | length) <= 128) 'OLD_UI_ZIP_SCOPE_DRIFT'
  let selected = ($names | where {|n| $n starts-with 'duckdb-runtime/'})
  let expected = ($ledger.old_ui.assets | columns | each {|name| $"duckdb-runtime/($name)"} | sort)
  require (($selected | sort) == $expected) 'OLD_UI_ASSET_SCOPE_DRIFT'
  let ui_parent = ($consumer | path join 'app' 'leptos-ui')
  let ui_info = (try { ls -lD $ui_parent } catch { [] })
  require (($ui_info | length) == 1 and ($ui_info | first).type == 'dir') 'RUNTIME_PARENT_DRIFT'
  let runtime = ($consumer | path join 'app' 'leptos-ui' 'duckdb-runtime')
  if not ($runtime | path exists) { mkdir $runtime }
  let runtime_info = (ls -lD $runtime)
  require (($runtime_info | length) == 1 and ($runtime_info | first).type == 'dir') 'RUNTIME_PARENT_DRIFT'
  for name in ($ledger.old_ui.assets | columns) {
    let target = ($runtime | path join $name)
    let info = (try { ls -lD $target } catch { [] })
    require (($info | is-empty) or (($info | length) == 1 and ($info | first).type == 'file')) 'RUNTIME_TARGET_DRIFT'
    let extraction = (^python3 -c 'import pathlib,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); i=z.getinfo(sys.argv[2]); assert 0 < i.file_size <= 67108864; pathlib.Path(sys.argv[3]).write_bytes(z.read(i))' $old_zip $"duckdb-runtime/($name)" $target | complete)
    require ($extraction.exit_code == 0) 'OLD_UI_ASSET_EXTRACT_FAILED'
    require ((digest $target) == ($ledger.old_ui.assets | get $name)) 'OLD_UI_ASSET_DRIFT'
  }
  let resources = ($consumer | path join 'app' 'tauri' 'third-party-licenses' 'ui' 'duckdb-runtime')
  let resource_info = (try { ls -lD $resources } catch { [] })
  require (($resource_info | length) == 1 and ($resource_info | first).type == 'dir') 'RESOURCE_PARENT_DRIFT'
  let notice = ($resources | path join 'native-emscripten-LICENSE')
  let archive = ($resources | path join 'native-emscripten-libnoexit.a')
  let cell_json = ($resources | path join 'native-emscripten-libnoexit-cell.json')
  require (not ($archive | path exists) and not ($cell_json | path exists)) 'SIDECAR_PREEXISTS'
  let notice_info = (try { ls -lD $notice } catch { [] })
  require (($notice_info | is-empty) or (($notice_info | length) == 1 and ($notice_info | first).type == 'file')) 'NOTICE_TARGET_DRIFT'
  let notice_source = ($authenticated | path join 'emscripten-LICENSE')
  let archive_source = ($authenticated | path join $"($condition)-libnoexit.a")
  require ((digest $notice_source) == $ledger.notice_sha256 and (digest $archive_source) == $ledger.archive_sha256) 'PREFLIGHT_SOURCE_BYTES_DRIFT'
  ^cp $notice_source $notice
  require ($env.LAST_EXIT_CODE == 0) 'NOTICE_STAGE_FAILED'
  require ((digest $notice) == $ledger.notice_sha256) 'NOTICE_STAGE_DRIFT'
  ^cp $archive_source $archive
  require ($env.LAST_EXIT_CODE == 0) 'ARCHIVE_STAGE_FAILED'
  require ((digest $archive) == $ledger.archive_sha256) 'ARCHIVE_STAGE_DRIFT'
  let cell = {
    schema: 'tundra-sdk-libnoexit-installed-resource-cell/v1', status: 'DIAGNOSTIC_PACKAGE_RESOURCE_ONLY',
    condition: $condition, profile: $profile, key: $"($condition)/sdk/libnoexit.a/__archive__/1",
    consumer_head: $ledger.consumer_head, old_ui_run: $ledger.old_ui.run,
    old_ui_artifact_id: $ledger.old_ui.artifact_id, old_ui_zip_sha256: $ledger.old_ui.zip_sha256,
    producer_run: $ledger.producer_run, producer_artifact_id: $ledger.producer_artifact_id,
    producer_zip_sha256: $ledger.producer_zip_sha256, archive_sha256: $ledger.archive_sha256,
    object_sha256: $ledger.object_sha256, notice_sha256: $ledger.notice_sha256,
    archive_resource: $ledger.archive_resource, notice_resource: $ledger.notice_resource,
    source_notice_path: 'emscripten/LICENSE', installed_notice_path: $ledger.notice_resource,
    selected_notice_status: 'PACKAGE_RESOURCE_NOTICE_ASSOCIATION_DIAGNOSTIC_ONLY',
    archive_resource_role: 'DIAGNOSTIC_PACKAGE_RESOURCE_NOT_LINKER_INPUT',
    archive_linker_input: 'UNPROVEN', generator_input_closure: 'UNPROVEN',
    selected_graph_exhaustive: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'
  }
  $cell | to json --raw | save $cell_json
  $cell
}

export def package_entry [listing: list<string>, suffix: string] {
  let rows = ($listing | where {|line| $line ends-with $"/($suffix)"})
  require (($rows | length) == 1 and ($rows | first) starts-with '-') 'PACKAGE_RESOURCE_NOT_REGULAR'
}

def extracted [root: string, suffix: string] {
  let found = (^find $root -type f -path $"*/($suffix)" | complete)
  require ($found.exit_code == 0) 'EXTRACTED_FIND_FAILED'
  let paths = ($found.stdout | lines)
  require (($paths | length) == 1) 'EXTRACTED_RESOURCE_SCOPE_DRIFT'
  let path = ($paths | first)
  regular $path 'EXTRACTED_RESOURCE_NOT_REGULAR' | ignore
  $path
}

export def validate_cell [ledger: record, cell: record, condition: string, profile: string] {
  require ($cell.schema == 'tundra-sdk-libnoexit-installed-resource-cell/v1' and $cell.status == 'DIAGNOSTIC_PACKAGE_RESOURCE_ONLY') 'CELL_SCHEMA_DRIFT'
  require ($cell.condition == $condition and $cell.profile == $profile and $cell.key == $"($condition)/sdk/libnoexit.a/__archive__/1") 'CELL_IDENTITY_DRIFT'
  require ($cell.consumer_head == $ledger.consumer_head and $cell.old_ui_run == $ledger.old_ui.run and $cell.old_ui_artifact_id == $ledger.old_ui.artifact_id and $cell.old_ui_zip_sha256 == $ledger.old_ui.zip_sha256) 'CELL_RUNTIME_BASIS_DRIFT'
  require ($cell.producer_run == $ledger.producer_run and $cell.producer_artifact_id == $ledger.producer_artifact_id and $cell.producer_zip_sha256 == $ledger.producer_zip_sha256) 'CELL_PRODUCER_DRIFT'
  require ($cell.archive_sha256 == $ledger.archive_sha256 and $cell.object_sha256 == $ledger.object_sha256 and $cell.notice_sha256 == $ledger.notice_sha256) 'CELL_BYTES_DRIFT'
  require ($cell.archive_resource == $ledger.archive_resource and $cell.notice_resource == $ledger.notice_resource) 'CELL_RESOURCE_DRIFT'
  require ($cell.source_notice_path == 'emscripten/LICENSE' and $cell.installed_notice_path == $ledger.notice_resource and $cell.selected_notice_status == 'PACKAGE_RESOURCE_NOTICE_ASSOCIATION_DIAGNOSTIC_ONLY') 'CELL_NOTICE_ASSOCIATION_DRIFT'
  require ($cell.archive_resource_role == 'DIAGNOSTIC_PACKAGE_RESOURCE_NOT_LINKER_INPUT') 'CELL_ARCHIVE_ROLE_DRIFT'
  require ($cell.archive_linker_input == 'UNPROVEN' and $cell.generator_input_closure == 'UNPROVEN' and $cell.selected_graph_exhaustive == 'NO') 'CELL_CLOSURE_DRIFT'
  for key in ['legal_admission' 'product_admission' 'fleet_admission'] {
    require (($cell | get $key) == 'NO') 'CELL_ADMISSION_DRIFT'
  }
  $cell
}

export def verify [ledger: record, preflight: record, consumer: string, condition: string, profile: string, deb: string, extracted_root: string, output: string] {
  fixed $ledger $preflight $condition $profile
  source_root $consumer
  require (not ($output | path exists)) 'OUTPUT_PREEXISTS'
  let size = (((regular $deb 'DEB_NOT_REGULAR').size) | into int)
  require ($size > 0 and $size <= $ledger.max_deb_bytes) 'DEB_CAP'
  let package_sha = (digest $deb)
  require ($package_sha not-in $ledger.retired_package_sha256) 'RETIRED_PACKAGE_SUBSTITUTION'
  let package_listing = (^dpkg-deb --contents $deb | complete)
  require ($package_listing.exit_code == 0) 'DEB_LIST_FAILED'
  require (($package_listing.stdout | str length) <= 2097152) 'DEB_LIST_CAP'
  let listing = ($package_listing.stdout | lines)
  require (($listing | length) <= 20000) 'DEB_LIST_CAP'
  for suffix in [$ledger.notice_resource $ledger.archive_resource $ledger.cell_resource] {
    package_entry $listing $suffix
  }
  let root_info = (try { ls -lD $extracted_root } catch { [] })
  require (($root_info | length) == 1 and ($root_info | first).type == 'dir') 'EXTRACTED_ROOT_DRIFT'
  let notice = (extracted $extracted_root $ledger.notice_resource)
  let archive = (extracted $extracted_root $ledger.archive_resource)
  let installed_cell = (extracted $extracted_root $ledger.cell_resource)
  require (($notice | path dirname) == ($archive | path dirname) and ($notice | path dirname) == ($installed_cell | path dirname)) 'EXTRACTED_RESOURCE_PARENT_DRIFT'
  require ((digest $notice) == $ledger.notice_sha256 and (digest $archive) == $ledger.archive_sha256) 'EXTRACTED_BYTES_DRIFT'
  let source_resources = ($consumer | path join 'app' 'tauri')
  require ((digest ($source_resources | path join $ledger.notice_resource)) == $ledger.notice_sha256) 'SOURCE_NOTICE_DRIFT'
  require ((digest ($source_resources | path join $ledger.archive_resource)) == $ledger.archive_sha256) 'SOURCE_ARCHIVE_DRIFT'
  require ((digest ($source_resources | path join $ledger.cell_resource)) == (digest $installed_cell)) 'EXTRACTED_CELL_DRIFT'
  let cell = (validate_cell $ledger (open $installed_cell) $condition $profile)
  let result = {
    schema: 'tundra-sdk-libnoexit-installed-cell-witness/v1', status: 'PACKAGE_EXTRACTED_RESOURCE_BYTES_MATCH_DIAGNOSTIC_ONLY',
    condition: $condition, profile: $profile, key: $cell.key,
    package_sha256: $package_sha, package_bytes: $size,
    cell_resource_sha256: (digest $installed_cell), archive_sha256: $ledger.archive_sha256,
    notice_sha256: $ledger.notice_sha256, producer_run: $ledger.producer_run,
    producer_artifact_id: $ledger.producer_artifact_id, producer_zip_sha256: $ledger.producer_zip_sha256,
    package_extraction_verified: 'YES', package_install_verified: 'NO',
    archive_file_in_package: 'YES_DIAGNOSTIC_RESOURCE', linker_input: 'UNPROVEN',
    generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO',
    legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'
  }
  mkdir $output
  ^cp $notice ($output | path join 'installed-notice')
  require ($env.LAST_EXIT_CODE == 0) 'RECEIPT_COPY_FAILED'
  ^cp $archive ($output | path join 'installed-archive')
  require ($env.LAST_EXIT_CODE == 0) 'RECEIPT_COPY_FAILED'
  ^cp $installed_cell ($output | path join 'installed-cell.json')
  require ($env.LAST_EXIT_CODE == 0) 'RECEIPT_COPY_FAILED'
  $result | to json --raw | save ($output | path join 'cell-witness.json')
  let receipt_size = ((^du -sb $output | split row (char tab) | first) | into int)
  require ($receipt_size <= $ledger.max_receipt_bytes) 'RECEIPT_CAP'
  $result
}

def main [mode: string, ...args: string] {
  match $mode {
    'prepare' => {
      require (($args | length) == 7) 'PREPARE_ARGUMENTS'
      prepare (open $args.0) (open $args.1) $args.2 $args.3 $args.4 $args.5 $args.6 | ignore
    }
    'verify' => {
      require (($args | length) == 8) 'VERIFY_ARGUMENTS'
      verify (open $args.0) (open $args.1) $args.2 $args.3 $args.4 $args.5 $args.6 $args.7 | ignore
    }
    _ => { error make {msg: 'UNKNOWN_MODE'} }
  }
}
