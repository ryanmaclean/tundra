#!/usr/bin/env nu

def require [ok: bool, reason: string] {
  if not $ok { error make {msg: $reason} }
}

def digest [path: string] {
  require ($path | path exists) 'FILE_MISSING'
  let item = (ls -l $path | first)
  require ($item.type == 'file') 'FILE_NOT_REGULAR'
  let result = (^sha256sum $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

export def select_compile_command [log: string] {
  require (($log | str length) <= 131072) 'BUILD_LOG_CAP'
  let expected_source = '../../../../../emsdk/upstream/emscripten/system/lib/libc/atexit_dummy.c'
  let matches = ($log | lines | where {|line|
    let trimmed = ($line | str trim)
    (($trimmed | str starts-with '/emsdk/upstream/emscripten/emcc ') and ($trimmed | str contains 'atexit_dummy.c'))
  })
  require (($matches | length) == 1) 'COMPILER_COMMAND_COUNT'
  let command = ($matches | first | str trim)
  require (($command | str length) <= 8192) 'COMPILER_COMMAND_CAP'
  # shared.print_compiler_stage uses shlex_join. The selected one-source command
  # needs no shell quoting; refuse other lexical forms instead of guessing argv.
  require ($command =~ '^[A-Za-z0-9_./=:+ -]+$') 'COMPILER_ARGV_AMBIGUOUS'
  let argv = ($command | split row ' ')
  require (($argv | str join ' ') == $command and not ($argv | any {|arg| $arg == ''})) 'COMPILER_ARGV_AMBIGUOUS'
  # Pinned system_libs.py builds libnoexit directly from Library: inherited
  # cflags, get_base_cflags, deterministic path flags, then its sole src_file.
  # An exact argv tuple rejects extra source operands and option operands.
  let expected = [
    '/emsdk/upstream/emscripten/emcc' '-c' '-O2' '-Wall' '-fno-unroll-loops'
    '-g' '-sSTRICT' '-Werror'
    '-ffile-prefix-map=/emsdk/upstream/emscripten=/emsdk/emscripten'
    '-fdebug-compilation-dir=/emsdk/emscripten' $expected_source
  ]
  require ($argv == $expected) 'COMPILER_ARGV_DRIFT'
  $command
}

export def validate_condition_shape [record: record, name: string, cache: string] {
  require ($record.condition == $name) 'CONDITION_DRIFT'
  require ($record.cache == $cache) 'CACHE_PATH_DRIFT'
  require ($record.member == 'atexit_dummy.o' and $record.ordinal == 1 and $record.member_count == 1) 'MEMBER_SCOPE_DRIFT'
  require ($record.source_sha256 == '7d085afbb2951dad60c4567319ae5299b669dc261767ea11f8760981c3324fb9') 'SOURCE_DRIFT'
  require ($record.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'NOTICE_DRIFT'
  require ($record.build_exit == 0 and $record.archive_bytes > $record.object_bytes and $record.object_bytes > 0) 'OBJECT_PRODUCTION_UNPROVEN'
  require ($record.archive_sha256 =~ '^[0-9a-f]{64}$' and $record.object_sha256 =~ '^[0-9a-f]{64}$') 'OUTPUT_HASH_SHAPE'
  require ($record.archive_sha256 != $record.object_sha256) 'ARCHIVE_OBJECT_HASH_COLLISION'
  require ($record.command_sha256 == ($record.command | hash sha256)) 'COMPILER_COMMAND_HASH_DRIFT'
  require ($record.cache_initially_empty == 'YES') 'CACHE_PRESTATE_UNPROVEN'
  true
}

def build_condition [name: string, cache: string, output: string, ledger: record] {
  require ($name in ['image' 'clean']) 'CONDITION_INVALID'
  require (not ($cache | path exists)) 'CACHE_PREEXISTS'
  let parent = ($cache | path dirname)
  mkdir $parent
  require (not ($cache | path exists)) 'CACHE_PREEXISTS'
  mkdir $cache
  require (((ls -a $cache) | length) == 0) 'CACHE_NOT_EMPTY'
  let invocation = (with-env {EM_CACHE: $cache, EMCC_VERBOSE: '1', EMCC_USE_NINJA: '0', HOME: '/tmp'} {
    ^timeout 300s python3 $ledger.embuilder_path build libnoexit | complete
  })
  let build_exit = $invocation.exit_code
  require ($build_exit == 0) 'EMBUILDER_FAILED'
  let log = $"($invocation.stdout)\n($invocation.stderr)"
  require (($log | str length) > 0 and ($log | str length) <= 131072) 'BUILD_LOG_CAP'
  let command = (select_compile_command $log)
  let archive = $"($cache)/($ledger.expected_archive_suffix)"
  require (($archive | path exists) and ((ls -l $archive | first).type == 'file')) 'ARCHIVE_MISSING'
  let members = (^/emsdk/upstream/bin/llvm-ar t $archive | lines)
  require ($members == [$ledger.expected_member]) 'MEMBER_SCOPE_DRIFT'
  let member_out = $"($output)/($name)-atexit_dummy.o"
  ^/emsdk/upstream/bin/llvm-ar p $archive $ledger.expected_member o> $member_out
  require ($env.LAST_EXIT_CODE == 0) 'MEMBER_EXTRACT_FAILED'
  let archive_out = $"($output)/($name)-libnoexit.a"
  cp $archive $archive_out
  require ((digest $archive) == (digest $archive_out)) 'ARCHIVE_COPY_DRIFT'
  let record = {
    condition: $name, cache: $cache, cache_initially_empty: 'YES',
    build_exit: $build_exit, command: $command, command_sha256: ($command | hash sha256),
    source_sha256: (digest $ledger.source_path), notice_sha256: (digest $ledger.notice_path),
    archive_sha256: (digest $archive_out), object_sha256: (digest $member_out),
    member: $ledger.expected_member, ordinal: 1, member_count: 1,
    archive_bytes: ((ls -l $archive_out | first).size | into int),
    object_bytes: ((ls -l $member_out | first).size | into int)
  }
  # A one-member archive is not byte-identical to its contained object.
  validate_condition_shape $record $name $cache | ignore
  $record
}

def main [ledger_path: string, output: string] {
  let ledger = (open $ledger_path)
  require ($ledger.schema == 'tundra-sdk-libnoexit-controlled-object/v2') 'LEDGER_SCHEMA'
  require ($ledger.scope == 'NEW_OBJECT_PRODUCTION_ONLY') 'LEDGER_SCOPE'
  require ($ledger.conditions == [{name: image, cache: '/work/image/cache'} {name: clean, cache: '/work/clean/cache'}]) 'LEDGER_CONDITIONS'
  require ($ledger.object_input_closure == 'UNPROVEN' and $ledger.sdk_archive_keys_pending_per_condition == 9) 'LEDGER_ADMISSION'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] {
    require (($ledger | get $key) == 'NO') 'LEDGER_ADMISSION'
  }
  require ($ledger.generator_input_closure == 'UNPROVEN' and $ledger.selected_graph_exhaustive == 'NO') 'LEDGER_ADMISSION'
  require ((digest $ledger.source_path) == $ledger.source_sha256) 'SOURCE_DRIFT'
  require ((digest $ledger.recipe_path) == $ledger.recipe_sha256) 'RECIPE_DRIFT'
  require ((digest $ledger.embuilder_path) == $ledger.embuilder_sha256) 'EMBUILDER_DRIFT'
  require ((digest $ledger.notice_path) == $ledger.notice_sha256) 'NOTICE_DRIFT'
  for path in $ledger.tool_paths { digest $path | ignore }
  let tool_hashes = ($ledger.tool_paths | each {|p| {path: $p, sha256: (digest $p)} })
  let image = (build_condition image '/work/image/cache' $output $ledger)
  let clean = (build_condition clean '/work/clean/cache' $output $ledger)
  let result = {
    schema: 'tundra-sdk-libnoexit-controlled-object/v2', status: 'PRODUCED_DIAGNOSTIC_ONLY',
    image_digest: $ledger.image, source_commit: $ledger.official_source_commit,
    recipe_sha256: $ledger.recipe_sha256, embuilder_sha256: $ledger.embuilder_sha256,
    tool_hashes: $tool_hashes, conditions: [$image $clean],
    source_notice_sha256: $ledger.notice_sha256, selected_license: 'MIT',
    old_producer_run: $ledger.old_producer_run,
    old_producer_bill_sha256: $ledger.old_producer_bill_sha256,
    object_input_closure: 'UNPROVEN', sdk_archive_keys_pending_per_condition: 9,
    generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO',
    glue_legal_admission: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'
  }
  $result | to json --raw | save $"($output)/controlled-object.json"
  cp $ledger.notice_path $"($output)/emscripten-LICENSE"
  rm $"($output)/refusal.json"
}
