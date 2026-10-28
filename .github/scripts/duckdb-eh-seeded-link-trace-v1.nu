#!/usr/bin/env nu

# Held same-run joins around the strict PR150 FD parser. No raw trace is emitted.
use ./duckdb-eh-seeded-build-v1.nu map_selection
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-eh-seeded-link-trace/v1' and $l.status == 'HELD_SOURCE_ONLY' and $l.condition == 'image') 'LEDGER_SCOPE'
  require ($l.archive_path == '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a' and $l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.archive_member_sha256 == 'd58194aea8fda6a000d2aa479617837afc6a7d527dcc670060bd7faade9d2ea1') 'LEDGER_ARCHIVE'
  require ($l.configured_linker == '/emsdk/upstream/bin/wasm-ld' and $l.resolved_linker == '/emsdk/upstream/bin/lld' and $l.linker_elf_sha256 == '6e5c12aab29b80db118acd16c08953257683ed11c415970f8320ed777997eec7') 'LEDGER_LINKER'
  require ($l.tracer_package == 'strace=5.16-0ubuntu3:amd64' and $l.tracer_elf_sha256 == '38a5c75cb29dd85ddd7780d54f5bf595554d7a1b5c42524b23065f5dc4c4b01d') 'LEDGER_TRACER'
  require ($l.libunwind_package == 'libunwind8=1.3.2-2build2.1:amd64' and $l.libunwind_deb_sha256 == '781e3eb86986ab93e8b4866fc571fdec780c3d9a8b56b490e9c2f10fb90978ce') 'LEDGER_LIBUNWIND'
  require ($l.host_loader_sha256 == 'e833bd99168bea158d2f2e5d32462479cacfb624003304a9ea74a1607a144f1f' and $l.host_libc_sha256 == '2e15e345ccba1e1e8bf3bc58938d13eb7ccc17044942f0376cd5bc771f429b79' and $l.host_liblzma_sha256 == '493cb401ab4aa3bba611ca464d12996afb3b327940d29476f535f999e167439b') 'LEDGER_LOADER_ABI'
  require ($l.linker_archive_read == 'UNPROVEN' and $l.other_sdk_archive_keys_pending == 8 and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_ADMISSION'
  $l
}

def info [path: string, cap: int] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file' and (($rows | first).size | into int) > 0 and (($rows | first).size | into int) <= $cap) 'FILE_TYPE_OR_SIZE'
  let hash = (^sha256sum -- $path | complete)
  let stat = (^stat -c '%d:%i:%s:%a' -- $path | complete)
  let real = (^readlink -f -- $path | complete)
  require ($hash.exit_code == 0 and $stat.exit_code == 0 and $real.exit_code == 0 and ($real.stdout | str trim) == $path) 'FILE_IDENTITY'
  {path: $path, bytes: (($rows | first).size | into int), sha256: ($hash.stdout | split row ' ' | first), stat: ($stat.stdout | str trim)}
}

def archive [l: record, private_dir: string] {
  let id = (info $l.archive_path 2048)
  require ($id.bytes == 1644 and $id.sha256 == $l.archive_sha256) 'ARCHIVE_IDENTITY'
  let names = (^/emsdk/upstream/bin/llvm-ar t $l.archive_path | complete)
  require ($names.exit_code == 0 and $names.stdout == "atexit_dummy.o\n") 'ARCHIVE_MEMBER_LIST'
  let member_path = ($private_dir | path join 'atexit_dummy.o')
  require (not ($member_path | path exists)) 'MEMBER_SCRATCH_OCCUPIED'
  ^/emsdk/upstream/bin/llvm-ar p $l.archive_path atexit_dummy.o o> $member_path
  require ($env.LAST_EXIT_CODE == 0) 'ARCHIVE_MEMBER_EXTRACT'
  let member = (info $member_path 2048)
  require ($member.bytes == 1463 and $member.sha256 == $l.archive_member_sha256) 'ARCHIVE_MEMBER_HASH'
  rm $member_path
  {archive: $id, member_sha256: $member.sha256, member_ordinal: 1, member_count: 1}
}

def paths [] {
  let root = $env.GITHUB_WORKSPACE
  {link: ($root | path join 'duckdb-upstream/build/integrated-eh-seeded-image/CMakeFiles/duckdb_wasm.dir/link.txt'),
   objects1: ($root | path join 'duckdb-upstream/build/integrated-eh-seeded-image/CMakeFiles/duckdb_wasm.dir/objects1.rsp'),
   linklibs: ($root | path join 'duckdb-upstream/build/integrated-eh-seeded-image/CMakeFiles/duckdb_wasm.dir/linklibs.rsp'),
   map: '/work/image/build-output/image.map',
   js: ($root | path join 'duckdb-upstream/build/integrated-eh-seeded-image/duckdb_wasm.js'),
   wasm: ($root | path join 'duckdb-upstream/build/integrated-eh-seeded-image/duckdb_wasm.wasm')}
}

def outputs [] {
  let p = (paths)
  {link: (info $p.link 262144), objects1: (info $p.objects1 20971520), linklibs: (info $p.linklibs 20971520), map: (info $p.map 104857600), js: (info $p.js 104857600), wasm: (info $p.wasm 314572800)}
}

export def canary_shape [raw: string, sentinel: string] {
  let lines = ($raw | lines)
  require (($lines | length) <= 10000 and ($lines | any {|line| ($line | str starts-with 'execve(') and ($line | str ends-with ') = 0')}) and ('+++ exited with 0 +++' in $lines)) 'CANARY_CHILD_EXIT'
  let opens = ($lines | each {|line| $line | parse -r '^openat\(AT_FDCWD, "(?<path>[^"\\]+)", O_RDONLY[^)]*\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$' } | flatten | where {|row| $row.path == $sentinel and $row.annotation == $sentinel })
  require (($opens | length) == 1) 'CANARY_DIRECT_OPEN'
  let fd = ($opens | first).fd
  let reads = ($lines | each {|line| $line | parse -r '^read\((?<fd>[0-9]+)<(?<annotation>[^>]+)>, .*\) = (?<count>[0-9]+)$' } | flatten | where {|row| $row.fd == $fd and $row.annotation == $sentinel and (($row.count | into int) > 0) })
  require (($reads | length) > 0) 'CANARY_DIRECT_READ'
  true
}

export def canary_diagnostic [raw: string, sentinel: string, result: string] {
  let lines = ($raw | lines)
  let exact_open_pattern = '^openat\(AT_FDCWD, "(?<path>[^"\\]+)", O_RDONLY[^)]*\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$'
  let exact_opens = ($lines | each {|line| $line | parse -r $exact_open_pattern } | flatten | where {|row| $row.path == $sentinel and $row.annotation == $sentinel })
  let failure = (if $result in [CANARY_CHILD_EXIT CANARY_DIRECT_OPEN CANARY_DIRECT_READ] { $result } else { 'CANARY_OTHER' })
  {
    failure: $failure,
    trace_lines: ($lines | length),
    openat_lines: ($lines | where {|line| $line | str starts-with 'openat(' } | length),
    open_lines: ($lines | where {|line| $line | str starts-with 'open(' } | length),
    openat2_lines: ($lines | where {|line| $line | str starts-with 'openat2(' } | length),
    sentinel_open_lines: ($lines | where {|line| ($line | str starts-with 'openat(') and ($line | str contains $sentinel) } | length),
    exact_sentinel_opens: ($exact_opens | length),
    read_lines: ($lines | where {|line| $line | str starts-with 'read(' } | length)
  }
}

export def canary [trace_dir: string, sentinel: string] {
  require ($trace_dir == '/work/image/trace-private' and $sentinel == '/work/image/trace-private/canary.txt') 'CANARY_SCOPE'
  let entries = (ls -l $trace_dir | where {|row| ($row.name | path basename) =~ '^canary\.trace\.[0-9]+$' })
  require (($entries | length) == 1 and ($entries | first).type == 'file' and (($entries | first).size | into int) > 0 and (($entries | first).size | into int) <= 1048576) 'CANARY_TRACE_SCOPE'
  let raw = (open --raw ($entries | first).name)
  let result = (try { canary_shape $raw $sentinel | ignore; 'PASS' } catch {|err| $err.msg })
  if $result != 'PASS' {
    let diagnostic = (canary_diagnostic $raw $sentinel $result)
    let refusal_path = '/work/image/receipt/refusal.json'
    let prior = (open $refusal_path)
    require ($prior.schema == 'tundra-eh-seeded-link-trace/v1' and $prior.status == 'REFUSED' and $prior.reason == 'TRACE_CANARY') 'CANARY_REFUSAL_SCOPE'
    $prior | merge {canary_diagnostic: $diagnostic} | to json --raw | save --force $refusal_path
    error make {msg: $diagnostic.failure}
  }
  print 'same-run tracer child canary: PASS'
}

export def pre [ledger_path: string, build_path: string, seed_path: string, auth_path: string, private_dir: string, output: string] {
  let l = (ledger $ledger_path)
  require ($env.EM_CACHE == '/work/image/cache' and $env.GITHUB_SHA != '' and $env.GITHUB_RUN_ID != '') 'RUN_SCOPE'
  let build = (open $build_path)
  let seed = (open $seed_path)
  let auth = (open $auth_path)
  require ($build.status == 'SEEDED_EH_BUILD_DIAGNOSTIC_ONLY' and $build.condition == 'image' and $build.diagnostic_head == $env.GITHUB_SHA and $build.producer_run_id == $l.producer_run_id and $build.producer_artifact_id == $l.producer_artifact_id and $build.producer_zip_sha256 == $l.producer_zip_sha256 and $build.image_member == $l.image_member) 'BUILD_RECEIPT_JOIN'
  require ($seed.status == 'HELD_SEED_IDENTITY_ONLY' and $seed.condition == 'image' and $seed.image_member_name == $l.image_member and $seed.settings_derived_path == $l.archive_path and $auth.status == 'PR143_ARTIFACT_AUTHENTICATED_ONLY') 'SEED_AUTH_JOIN'
  let ar = (archive $l $private_dir)
  require ($ar.archive.stat == $seed.seed.stat and $ar.archive.sha256 == $seed.seed.sha256) 'SEED_INODE_DRIFT'
  let out = (outputs)
  for key in [link objects1 linklibs map js wasm] {
    let observed = ($out | get $key)
    let prior = ($build.outputs | get $key)
    require ($observed.path == $prior.path and $observed.bytes == $prior.bytes and $observed.sha256 == $prior.sha256) 'BUILD_OUTPUT_DRIFT'
  }
  map_selection $out.map.path $l.archive_path | ignore
  {schema: $l.schema, phase: 'PRE_TRACE', run_id: $env.GITHUB_RUN_ID, attempt: $env.GITHUB_RUN_ATTEMPT, head: $env.GITHUB_SHA, archive: $ar, outputs: $out} | to json --raw | save $output
}

export def absent [pre_path: string, output: string] {
  let before = (open $pre_path)
  require ($before.phase == 'PRE_TRACE' and $before.run_id == $env.GITHUB_RUN_ID and $before.head == $env.GITHUB_SHA) 'PRE_TRACE_SCOPE'
  let p = (paths)
  require (not ($p.map | path exists) and not ($p.js | path exists) and not ($p.wasm | path exists)) 'OUTPUT_NOT_REMOVED'
  {schema: $before.schema, phase: 'OUTPUTS_ABSENT_BEFORE_TRACE', run_id: $env.GITHUB_RUN_ID, attempt: $env.GITHUB_RUN_ATTEMPT, head: $env.GITHUB_SHA, baseline_map_sha256: $before.outputs.map.sha256, baseline_js_sha256: $before.outputs.js.sha256, baseline_wasm_sha256: $before.outputs.wasm.sha256} | to json --raw | save $output
}

export def finalize [ledger_path: string, pre_path: string, absent_path: string, parser_path: string, private_dir: string, output: string] {
  let l = (ledger $ledger_path)
  let before = (open $pre_path)
  let removal = (open $absent_path)
  let shape = (open $parser_path)
  require ($before.schema == $l.schema and $before.phase == 'PRE_TRACE' and $before.run_id == $env.GITHUB_RUN_ID and $before.attempt == $env.GITHUB_RUN_ATTEMPT and $before.head == $env.GITHUB_SHA) 'PRE_TRACE_SCOPE'
  require ($removal.phase == 'OUTPUTS_ABSENT_BEFORE_TRACE' and $removal.run_id == $before.run_id and $removal.attempt == $before.attempt and $removal.head == $before.head and $removal.baseline_map_sha256 == $before.outputs.map.sha256 and $removal.baseline_js_sha256 == $before.outputs.js.sha256 and $removal.baseline_wasm_sha256 == $before.outputs.wasm.sha256) 'OUTPUT_REMOVAL_JOIN'
  require ($shape.status == 'FIXTURE_DIRECT_READ_SHAPE' and $shape.configured_linker_path == $l.configured_linker and $shape.linker_elf_sha256 == $l.linker_elf_sha256 and $shape.archive_sha256 == $l.archive_sha256 and $shape.linker_pid > 0 and $shape.direct_archive_opens > 0 and $shape.positive_reads > 0 and $shape.trace_file_count <= $l.max_trace_files and $shape.trace_bytes <= $l.max_total_trace_bytes) 'PARSER_DIRECT_READ_JOIN'
  let ar = (archive $l $private_dir)
  require ($ar == $before.archive) 'ARCHIVE_CHANGED_ACROSS_LINK'
  let out = (outputs)
  for key in [link objects1 linklibs map js wasm] {
    let observed = ($out | get $key)
    let prior = ($before.outputs | get $key)
    require ($observed.path == $prior.path and $observed.bytes == $prior.bytes and $observed.sha256 == $prior.sha256) 'DIFF_SAME_RUN_REPLAY'
  }
  map_selection $out.map.path $l.archive_path | ignore
  {schema: $l.schema, status: 'SEEDED_IMAGE_EH_DIRECT_FD_READ_DIAGNOSTIC_ONLY', condition: 'image', run_id: $env.GITHUB_RUN_ID, run_attempt: $env.GITHUB_RUN_ATTEMPT, diagnostic_head: $env.GITHUB_SHA, producer_run_id: $l.producer_run_id, producer_artifact_id: $l.producer_artifact_id, image_member: $l.image_member, linker: {configured_path: $l.configured_linker, resolved_path: $l.resolved_linker, elf_sha256: $l.linker_elf_sha256, pid: $shape.linker_pid}, archive_before_after: $ar, direct_archive_opens: $shape.direct_archive_opens, positive_archive_reads: $shape.positive_reads, parser_trace_files: $shape.trace_file_count, parser_trace_bytes: $shape.trace_bytes, baseline_outputs: $before.outputs, traced_outputs: $out, map_selection_class: 'ONE_EXACT_SEEDED_PATH_LIBNOEXIT_ATEXIT_DUMMY_CXA_ATEXIT_ROW', raw_trace_retained: 'PENDING_CLEANUP', actual_eh_linker_child: 'OBSERVED_EXECVE_ZERO_EXIT', linker_archive_read: 'OBSERVED_DIRECT_FD_POSITIVE_READ', same_run_traced_outputs: 'OBSERVED_FRESH_MAP_JS_WASM', other_sdk_archive_keys_pending: 8, object_input_closure: 'UNPROVEN', generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $output
}

def main [mode: string, first: string, second: string, third?: string, fourth?: string, fifth?: string, sixth?: string] {
  if $mode == 'canary' {
    canary $first $second
  } else if $mode == 'pre' {
    require ($third != null and $fourth != null and $fifth != null and $sixth != null) 'PRE_ARGS'
    pre $first $second $third $fourth $fifth $sixth
  } else if $mode == 'absent' {
    absent $first $second
  } else if $mode == 'finalize' {
    require ($third != null and $fourth != null and $fifth != null and $sixth != null) 'FINAL_ARGS'
    finalize $first $second $third $fourth $fifth $sixth
  } else { error make {msg: 'MODE'} }
}
