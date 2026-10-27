#!/usr/bin/env nu

def require [ok: bool, reason: string] {
  if not $ok { error make {msg: $reason} }
}

export def digest [path: string, stage: string] {
  require ($stage in ['SOURCE' 'RECIPE' 'EMBUILDER' 'NOTICE' 'TOOL_EMCC' 'TOOL_CLANG' 'TOOL_LLVM_AR' 'ARCHIVE_INPUT' 'ARCHIVE_COPY' 'OBJECT_COPY']) 'DIGEST_STAGE_INVALID'
  require ($path | path exists) $"($stage)_FILE_MISSING"
  let item = (ls -l $path | first)
  require ($item.type == 'file') $"($stage)_FILE_NOT_REGULAR"
  let result = (^sha256sum $path | complete)
  require ($result.exit_code == 0) $"($stage)_HASH_FAILED"
  $result.stdout | split row ' ' | first
}

# This is a diagnostic for the exact pinned clang path, not a tool fallback.
# Each link target must be a basename in the same real directory. Refuse any
# absolute target outside that directory, slash, traversal, cycle, or >4 hops.
export def inspect_confined_tool_link [path: string, root: string] {
  require (($path | path dirname) == $root) 'TOOL_CLANG_PATH_DRIFT'
  let directory = (try { ls -lD $root } catch { [] })
  require (($directory | length) == 1 and ($directory | first).type == 'dir') 'TOOL_CLANG_ROOT_NOT_REGULAR_DIR'
  mut current = $path
  mut seen = []
  for hop in 0..4 {
    require (not ($current in $seen)) 'TOOL_CLANG_LINK_CYCLE'
    $seen = ($seen | append $current)
    let entries = (try { ls -lD $current } catch { [] })
    require (($entries | length) == 1) 'TOOL_CLANG_LINK_TARGET_MISSING'
    let item = ($entries | first)
    if $item.type == 'file' {
      require ($hop > 0) 'TOOL_CLANG_LINK_NOT_PRESENT'
      return {hops: $hop, target_regular: 'YES', confined: 'YES', target_sha256: (digest $current 'TOOL_CLANG')}
    }
    require ($item.type == 'symlink') 'TOOL_CLANG_LINK_TARGET_NOT_REGULAR'
    require ($hop < 4) 'TOOL_CLANG_LINK_DEPTH'
    let target = ($item.target | default '')
    let name = if ($target | str starts-with '/') {
      require ($target | str starts-with $"($root)/") 'TOOL_CLANG_LINK_ESCAPE'
      $target | str replace $"($root)/" ''
    } else { $target }
    require ($name =~ '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$') 'TOOL_CLANG_LINK_ESCAPE'
    $current = ($root | path join $name)
  }
  error make {msg: 'TOOL_CLANG_LINK_DEPTH'}
}

export def accept_pinned_clang [record: record] {
  require ($record.hops == 1 and $record.target_regular == 'YES' and $record.confined == 'YES') 'TOOL_CLANG_LINK_SHAPE_DRIFT'
  require ($record.target_sha256 == '1dc630418acfa9cb6bb3e0df4ba17ee81053e7a7f3ad1f8920df095a9c53c581') 'TOOL_CLANG_TARGET_DRIFT'
  $record.target_sha256
}

export def select_compile_command [log: string] {
  require (($log | str length) > 0 and ($log | str length) <= 131072) 'BUILD_LOG_CAP'
  let expected_source = '../../../../../emsdk/upstream/emscripten/system/lib/dlmalloc.c'
  let matches = ($log | lines | where {|line|
    ($line | str trim | str starts-with '/emsdk/upstream/emscripten/emcc ')
  })
  require (($matches | length) == 1) 'COMPILER_COMMAND_COUNT'
  let command = ($matches | first | str trim)
  require (($command | str length) <= 8192) 'COMPILER_COMMAND_CAP'
  # shared.print_compiler_stage uses shlex_join. The selected one-source command
  # needs no shell quoting; refuse other lexical forms instead of guessing argv.
  require ($command =~ '^[A-Za-z0-9_./=:+ -]+$') 'COMPILER_ARGV_AMBIGUOUS'
  let argv = ($command | split row ' ')
  require (($argv | str join ' ') == $command and not ($argv | any {|arg| $arg == ''})) 'COMPILER_ARGV_AMBIGUOUS'
  # Pinned ordinary libmalloc(MTLibrary) dlmalloc variation: inherited
  # cflags, base flags, non-debug define, deterministic path flags and the
  # sole source. The exact tuple refuses alternate malloc variations.
  # An exact argv tuple rejects extra source operands and option operands.
  let expected = [
    '/emsdk/upstream/emscripten/emcc' '-c' '-O2' '-Wall' '-fno-unroll-loops'
    '-fno-builtin' '-Wno-unused-function' '-Wno-unused-but-set-variable' '-Wno-unused-variable'
    '-g' '-sSTRICT' '-Werror' '-DNDEBUG'
    '-ffile-prefix-map=/emsdk/upstream/emscripten=/emsdk/emscripten'
    '-fdebug-compilation-dir=/emsdk/emscripten' $expected_source
  ]
  require ($argv == $expected) 'COMPILER_ARGV_DRIFT'
  $command
}

export def validate_condition_shape [record: record, name: string, cache: string] {
  require ($record.condition == $name) 'CONDITION_DRIFT'
  require ($record.cache == $cache) 'CACHE_PATH_DRIFT'
  require ($record.member == 'dlmalloc.o' and $record.ordinal == 1 and $record.member_count == 1) 'MEMBER_SCOPE_DRIFT'
  require ($record.source_sha256 == 'de077adcccfc9fe18d91a4dc4d7bcc501156a9d28e982635c95931c763be7c96') 'SOURCE_DRIFT'
  require ($record.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'NOTICE_DRIFT'
  require ($record.build_exit == 0 and $record.archive_bytes > $record.object_bytes and $record.object_bytes > 0) 'OBJECT_PRODUCTION_UNPROVEN'
  require ($record.archive_sha256 =~ '^[0-9a-f]{64}$' and $record.object_sha256 =~ '^[0-9a-f]{64}$') 'OUTPUT_HASH_SHAPE'
  require ($record.archive_sha256 != $record.object_sha256) 'ARCHIVE_OBJECT_HASH_COLLISION'
  require ($record.command_sha256 == ($record.command | hash sha256)) 'COMPILER_COMMAND_HASH_DRIFT'
  require ($record.cache_initially_empty == 'YES') 'CACHE_PRESTATE_UNPROVEN'
  true
}

export def select_member_list [result: record, expected: string] {
  require ($result.exit_code == 0) 'MEMBER_LIST_FAILED'
  let members = ($result.stdout | lines)
  require ($members == [$expected]) 'MEMBER_SCOPE_DRIFT'
  $members
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
    ^timeout 300s python3 $ledger.embuilder_path build libdlmalloc | complete
  })
  let build_exit = $invocation.exit_code
  require ($build_exit == 0) 'EMBUILDER_FAILED'
  let log = $"($invocation.stdout)\n($invocation.stderr)"
  require (($log | str length) > 0 and ($log | str length) <= 131072) 'BUILD_LOG_CAP'
  let command = (select_compile_command $log)
  let archive = $"($cache)/($ledger.expected_archive_suffix)"
  require (($archive | path exists) and ((ls -l $archive | first).type == 'file')) 'ARCHIVE_MISSING'
  let ar_result = (^/emsdk/upstream/bin/llvm-ar t $archive | complete)
  select_member_list $ar_result $ledger.expected_member | ignore
  let member_out = $"($output)/($name)-dlmalloc.o"
  ^/emsdk/upstream/bin/llvm-ar p $archive $ledger.expected_member o> $member_out
  require ($env.LAST_EXIT_CODE == 0) 'MEMBER_EXTRACT_FAILED'
  let archive_out = $"($output)/($name)-libdlmalloc.a"
  cp $archive $archive_out
  require ((digest $archive 'ARCHIVE_INPUT') == (digest $archive_out 'ARCHIVE_COPY')) 'ARCHIVE_COPY_DRIFT'
  let record = {
    condition: $name, cache: $cache, cache_initially_empty: 'YES',
    build_exit: $build_exit, command: $command, command_sha256: ($command | hash sha256),
    source_sha256: (digest $ledger.source_path 'SOURCE'), notice_sha256: (digest $ledger.notice_path 'NOTICE'),
    archive_sha256: (digest $archive_out 'ARCHIVE_COPY'), object_sha256: (digest $member_out 'OBJECT_COPY'),
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
  require ($ledger.schema == 'tundra-sdk-dlmalloc-controlled-object/v1') 'LEDGER_SCHEMA'
  require ($ledger.scope == 'NEW_DLMALLOC_OBJECT_PRODUCTION_ONLY') 'LEDGER_SCOPE'
  require ($ledger.conditions == [{name: image, cache: '/work/image/cache'} {name: clean, cache: '/work/clean/cache'}]) 'LEDGER_CONDITIONS'
  require ($ledger.object_input_closure == 'UNPROVEN' and $ledger.sdk_archive_keys_pending_per_condition == 9) 'LEDGER_ADMISSION'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] {
    require (($ledger | get $key) == 'NO') 'LEDGER_ADMISSION'
  }
  require ($ledger.generator_input_closure == 'UNPROVEN' and $ledger.selected_graph_exhaustive == 'NO') 'LEDGER_ADMISSION'
  require ($ledger.tool_paths == ['/emsdk/upstream/emscripten/emcc' '/emsdk/upstream/bin/clang' '/emsdk/upstream/bin/llvm-ar']) 'TOOL_PATHS_DRIFT'
  require ($ledger.expected_archive_suffix == 'sysroot/lib/wasm32-emscripten/libdlmalloc.a' and $ledger.expected_member == 'dlmalloc.o') 'ARCHIVE_SCOPE_DRIFT'
  require ($ledger.source_path == '/emsdk/upstream/emscripten/system/lib/dlmalloc.c' and $ledger.source_sha256 == 'de077adcccfc9fe18d91a4dc4d7bcc501156a9d28e982635c95931c763be7c96') 'SOURCE_SCOPE_DRIFT'
  require ($ledger.notice_path == '/emsdk/upstream/emscripten/LICENSE' and $ledger.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86' and $ledger.selected_license == 'MIT') 'NOTICE_SCOPE_DRIFT'
  require ($ledger.source_origin == 'DOUG_LEA_PUBLIC_DOMAIN_CC0_REFERENCE_SOURCE_ONLY' and $ledger.emscripten_modifications_license == 'MIT_OPTION_ROOT_LICENSE' and $ledger.installed_dlmalloc_attribution == 'UNPROVEN') 'SOURCE_ORIGIN_DRIFT'
  require ((digest $ledger.source_path 'SOURCE') == $ledger.source_sha256) 'SOURCE_DRIFT'
  require ((digest $ledger.recipe_path 'RECIPE') == $ledger.recipe_sha256) 'RECIPE_DRIFT'
  require ((digest $ledger.embuilder_path 'EMBUILDER') == $ledger.embuilder_sha256) 'EMBUILDER_DRIFT'
  require ((digest $ledger.notice_path 'NOTICE') == $ledger.notice_sha256) 'NOTICE_DRIFT'
  for directory in ['/emsdk' '/emsdk/upstream' '/emsdk/upstream/bin'] {
    let entries = (try { ls -lD $directory } catch { [] })
    require (($entries | length) == 1 and ($entries | first).type == 'dir') 'TOOL_CLANG_ANCESTOR_DRIFT'
  }
  let clang_sha = (accept_pinned_clang (inspect_confined_tool_link '/emsdk/upstream/bin/clang' '/emsdk/upstream/bin'))
  let emcc_sha = (digest '/emsdk/upstream/emscripten/emcc' 'TOOL_EMCC')
  let llvm_ar_sha = (digest '/emsdk/upstream/bin/llvm-ar' 'TOOL_LLVM_AR')
  require ($emcc_sha == $ledger.expected_tool_sha256.emcc and $clang_sha == $ledger.expected_tool_sha256.clang and $llvm_ar_sha == $ledger.expected_tool_sha256.llvm_ar) 'TOOL_HASH_DRIFT'
  let tool_hashes = [
    {path: '/emsdk/upstream/emscripten/emcc', sha256: $emcc_sha}
    {path: '/emsdk/upstream/bin/clang', sha256: $clang_sha}
    {path: '/emsdk/upstream/bin/llvm-ar', sha256: $llvm_ar_sha}
  ]
  let image = (build_condition image '/work/image/cache' $output $ledger)
  let clean = (build_condition clean '/work/clean/cache' $output $ledger)
  let result = {
    schema: 'tundra-sdk-dlmalloc-controlled-object/v1', status: 'PRODUCED_DIAGNOSTIC_ONLY',
    image_digest: $ledger.image, source_commit: $ledger.official_source_commit,
    recipe_sha256: $ledger.recipe_sha256, embuilder_sha256: $ledger.embuilder_sha256,
    tool_hashes: $tool_hashes, conditions: [$image $clean],
    source_sha256: $ledger.source_sha256, source_origin: $ledger.source_origin,
    emscripten_modifications_license: $ledger.emscripten_modifications_license,
    source_notice_sha256: $ledger.notice_sha256, selected_license: 'MIT',
    installed_dlmalloc_attribution: 'UNPROVEN',
    old_producer_run: $ledger.old_producer_run,
    old_producer_bill_sha256: $ledger.old_producer_bill_sha256,
    object_input_closure: 'UNPROVEN', sdk_archive_keys_pending_per_condition: 9,
    generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO',
    glue_legal_admission: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'
  }
  $result | to json --raw | save $"($output)/controlled-object.json"
  cp $ledger.source_path $"($output)/dlmalloc.c"
  require ((digest $"($output)/dlmalloc.c" 'SOURCE') == $ledger.source_sha256) 'SOURCE_COPY_DRIFT'
  cp $ledger.notice_path $"($output)/emscripten-LICENSE"
  require ((digest $"($output)/emscripten-LICENSE" 'NOTICE') == $ledger.notice_sha256) 'NOTICE_COPY_DRIFT'
  rm $"($output)/refusal.json"
}
