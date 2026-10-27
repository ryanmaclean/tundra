#!/usr/bin/env nu

# Fresh-cache physical-member diagnostic. Source/header and installed notice closure remain unproven.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

export def digest [path: string] {
  require ($path | path exists) 'FILE_MISSING'
  let item = (ls -lD $path | first)
  require ($item.type == 'file') 'FILE_NOT_REGULAR'
  let result = (^sha256sum -- $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  $result.stdout | split row ' ' | first
}

export def inspect_clang [path: string, root: string] {
  require (($path | path dirname) == $root) 'CLANG_PATH_DRIFT'
  require ((ls -lD $root | first).type == 'dir') 'CLANG_ROOT_DRIFT'
  mut current = $path
  mut seen = []
  for hop in 0..4 {
    require (not ($current in $seen)) 'CLANG_LINK_CYCLE'
    $seen = ($seen | append $current)
    let entries = (try { ls -lD $current } catch { [] })
    require (($entries | length) == 1) 'CLANG_LINK_MISSING'
    let item = ($entries | first)
    if $item.type == 'file' {
      require ($hop == 1) 'CLANG_LINK_SHAPE'
      let sha = (digest $current)
      require ($sha == '1dc630418acfa9cb6bb3e0df4ba17ee81053e7a7f3ad1f8920df095a9c53c581') 'CLANG_TARGET_DRIFT'
      return $sha
    }
    require ($item.type == 'symlink' and $hop < 4) 'CLANG_LINK_SHAPE'
    let target = ($item.target | default '')
    let name = if ($target | str starts-with '/') {
      require ($target | str starts-with $"($root)/") 'CLANG_LINK_ESCAPE'
      $target | str replace $"($root)/" ''
    } else { $target }
    require ($name =~ '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$') 'CLANG_LINK_ESCAPE'
    $current = ($root | path join $name)
  }
  error make {msg: 'CLANG_LINK_DEPTH'}
}

export def safe_tokens [text: string] {
  require (($text | str length) > 0 and ($text | str length) <= 65536) 'ARGV_CAP'
  let normalized = ($text | lines | each {|line| $line | str trim} | where {|line| $line != ''} | str join ' ')
  require ($normalized =~ '^[A-Za-z0-9_./=:+@ -]+$') 'ARGV_AMBIGUOUS'
  let tokens = ($normalized | split row ' ' | where {|token| $token != ''})
  require (($tokens | str join ' ') == $normalized) 'ARGV_AMBIGUOUS'
  $tokens
}

export def parse_batches [log: string, build_dir: string, sources: list] {
  require (($log | str length) > 0 and ($log | str length) <= 2097152) 'BUILD_LOG_CAP'
  let lines = ($log | lines | where {|line| $line | str trim | str starts-with '/emsdk/upstream/emscripten/emcc '})
  require (($lines | length) > 0 and ($lines | length) <= 21) 'COMPILER_BATCH_COUNT'
  let prefix = [
    '/emsdk/upstream/emscripten/emcc' '-c' '-O2' '-Wall' '-fno-unroll-loops'
    '-std=c99' '-D_XOPEN_SOURCE=700' '-Wno-unused-result'
    '-Os' '-fno-inline-functions' '-fno-builtin' '-Wno-shift-op-parentheses'
    '-g' '-sSTRICT' '-Werror'
    '-I/emsdk/upstream/emscripten/system/lib/libc/musl/src/internal'
    '-I/emsdk/upstream/emscripten/system/lib/libc/musl/src/include'
    '-I/emsdk/upstream/emscripten/system/lib/libc'
    '-I/emsdk/upstream/emscripten/system/lib/pthread'
    '-ffile-prefix-map=/emsdk/upstream/emscripten=/emsdk/emscripten'
    '-fdebug-compilation-dir=/emsdk/emscripten'
  ]
  let expected = ($sources | each {|row| {
    member: $row.member,
    source: ($row.path | str replace '/emsdk/upstream/emscripten/' '../../../../../emsdk/upstream/emscripten/')
  }})
  mut found = []
  mut batches = []
  for line in $lines {
    let raw = ($line | str trim)
    require (($raw | str length) <= 8192) 'COMPILER_COMMAND_CAP'
    let raw_argv = (safe_tokens $raw)
    mut argv = []
    mut response_sha = 'NONE'
    for token in $raw_argv {
      if ($token | str starts-with '@') {
        require ($response_sha == 'NONE') 'RESPONSE_DUPLICATE'
        let name = ($token | str substring 1..)
        require ($name =~ '^/tmp/emscripten_[A-Za-z0-9_-]{1,32}\.rsp\.utf-8$') 'RESPONSE_PATH_DRIFT'
        let path = $name
        $response_sha = (digest $path)
        let contents = (open --raw $path)
        $argv = ($argv | append (safe_tokens $contents))
      } else { $argv = ($argv | append $token) }
    }
    require (($argv | first ($prefix | length)) == $prefix) 'COMPILER_PREFIX_DRIFT'
    let operands = ($argv | skip ($prefix | length))
    require (($operands | length) > 0 and ($operands | length) <= 21) 'SOURCE_OPERAND_COUNT'
    let selected = ($operands | each {|operand|
      let match = ($expected | where source == $operand)
      require (($match | length) == 1) 'SOURCE_OPERAND_UNDECLARED'
      $match | first | get member
    })
    require (($selected | uniq | length) == ($selected | length)) 'SOURCE_OPERAND_DUPLICATE'
    $found = ($found | append $selected)
    $batches = ($batches | append {command_sha256: ($raw | hash sha256), response_sha256: $response_sha, compiler_argv_checked: 'YES', members: $selected})
  }
  let all = ($found | flatten)
  require (($all | length) == 21 and ($all | uniq | length) == 21) 'SOURCE_COVERAGE'
  require (($all | sort) == ($sources | get member | sort)) 'SOURCE_COVERAGE'
  $batches
}

export def validate_member_order [result: record, members: list] {
  require ($result.exit_code == 0) 'MEMBER_LIST_FAILED'
  require (($result.stdout | lines) == $members) 'MEMBER_ORDER_OR_SCOPE'
  true
}

def build_condition [name: string, cache: string, output: string, ledger: record] {
  require (not ($cache | path exists)) 'CACHE_PREEXISTS'
  mkdir ($cache | path dirname)
  require (not ($cache | path exists)) 'CACHE_PREEXISTS'
  mkdir $cache
  require (((ls -a $cache) | length) == 0) 'CACHE_NOT_EMPTY'
  let invocation = (with-env {EM_CACHE: $cache, EMCC_VERBOSE: '1', EMCC_DEBUG: '1', EMCC_USE_NINJA: '0', EM_FORCE_RESPONSE_FILES: '0', HOME: '/tmp'} {
    ^timeout 600s python3 $ledger.embuilder_path build libsockets | complete
  })
  require ($invocation.exit_code == 0) 'EMBUILDER_FAILED'
  let build_dir = $"($cache)/build/libsockets-tmp"
  require (($build_dir | path exists) and ((ls -lD $build_dir | first).type == 'dir')) 'BUILD_OBJECT_DIR_MISSING'
  let batches = (parse_batches $"($invocation.stdout)\n($invocation.stderr)" $build_dir $ledger.sources)
  let archive = $"($cache)/($ledger.expected_archive_suffix)"
  let ar_bytes = ((ls -lD $archive | first).size | into int)
  require ($ar_bytes > 0 and $ar_bytes <= 262144) 'ARCHIVE_CAP'
  let ar_list = (^/emsdk/upstream/bin/llvm-ar t $archive | complete)
  let members = ($ledger.sources | get member)
  validate_member_order $ar_list $members | ignore
  let archive_copy = $"($output)/($name)-libsockets.a"
  cp $archive $archive_copy
  require ((digest $archive) == (digest $archive_copy)) 'ARCHIVE_COPY_DRIFT'
  mkdir $"($output)/($name)-objects"
  mut rows = []
  mut object_total = 0
  for row in $ledger.sources {
    let built = ($build_dir | path join $row.member)
    let payload = $"($output)/($name)-objects/($row.member)"
    ^/emsdk/upstream/bin/llvm-ar p $archive $row.member o> $payload
    require ($env.LAST_EXIT_CODE == 0) 'MEMBER_EXTRACT_FAILED'
    let bytes = ((ls -lD $payload | first).size | into int)
    require ($bytes > 0) 'OBJECT_EMPTY'
    $object_total = $object_total + $bytes
    require ($object_total <= 262144) 'OBJECT_CAP'
    require ((digest $built) == (digest $payload)) 'BUILD_OBJECT_MEMBER_DRIFT'
    let matched = ($batches | where {|batch| $row.member in $batch.members})
    require (($matched | length) == 1) 'SOURCE_BATCH_AMBIGUOUS'
    $rows = ($rows | append {member: $row.member, ordinal: $row.ordinal, source_sha256: (digest $row.path), object_sha256: (digest $payload), object_bytes: $bytes, command_sha256: $matched.0.command_sha256, response_sha256: $matched.0.response_sha256})
  }
  {condition: $name, cache: $cache, cache_initially_empty: 'YES', build_exit: 0,
   archive_sha256: (digest $archive_copy), archive_bytes: $ar_bytes,
   member_count: 21, object_bytes_total: $object_total, batches: $batches, rows: $rows}
}

def main [ledger_path: string, output: string] {
  let ledger = (open $ledger_path)
  require ($ledger.schema == 'tundra-sdk-libsockets-controlled-object/v1' and $ledger.scope == 'NEW_LIBSOCKETS_OBJECT_PRODUCTION_ONLY') 'LEDGER_SCOPE'
  require ($ledger.conditions == [{name: image, cache: '/work/image/cache'} {name: clean, cache: '/work/clean/cache'}]) 'LEDGER_CONDITIONS'
  require (($ledger.sources | length) == 21 and ($ledger.sources | get ordinal) == (1..21 | each {|x| $x})) 'LEDGER_MEMBER_SCOPE'
  require (($ledger.sources | get member | uniq | length) == 21) 'LEDGER_MEMBER_SCOPE'
  require ($ledger.expected_archive_suffix == 'sysroot/lib/wasm32-emscripten/libsockets.a') 'LEDGER_ARCHIVE'
  require ($ledger.expected_member_count == 21 and $ledger.tool_paths == ['/emsdk/upstream/emscripten/emcc' '/emsdk/upstream/bin/clang' '/emsdk/upstream/bin/llvm-ar']) 'LEDGER_SCOPE'
  require ($ledger.recipe_sha256 == 'cfbe818b5f6fc59102e55eb2ef8da94ba9273a484d799fcac5fa831ed3c2ee07' and $ledger.embuilder_sha256 == '814bdeeedc848bb67b910a80e51dd017705df43728708b50117ed7cf4e34cad6') 'LEDGER_SOURCE'
  require ($ledger.emscripten_notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86' and $ledger.musl_notice_sha256 == 'f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af' and $ledger.selected_license == 'MIT_PLUS_MIT') 'LEDGER_NOTICE'
  require ($ledger.object_input_closure == 'UNPROVEN' and $ledger.sdk_archive_keys_pending_per_condition == 9 and $ledger.generator_input_closure == 'UNPROVEN' and $ledger.selected_graph_exhaustive == 'NO') 'LEDGER_ADMISSION'
  for key in ['glue_legal_admission' 'legal_admission' 'product_admission' 'fleet_admission'] { require (($ledger | get $key) == 'NO') 'LEDGER_ADMISSION' }
  require ((digest $ledger.recipe_path) == $ledger.recipe_sha256 and (digest $ledger.embuilder_path) == $ledger.embuilder_sha256) 'RECIPE_DRIFT'
  require ((digest $ledger.emscripten_notice_path) == $ledger.emscripten_notice_sha256 and (digest $ledger.musl_notice_path) == $ledger.musl_notice_sha256) 'NOTICE_DRIFT'
  for row in $ledger.sources {
    require ($row.path | str starts-with '/emsdk/upstream/emscripten/system/lib/libc/musl/src/network/') 'SOURCE_PATH_DRIFT'
    require ($row.path | str ends-with ($row.member | str replace -r '\.o$' '.c')) 'SOURCE_PATH_DRIFT'
    require ((digest $row.path) == $row.sha256) 'SOURCE_DRIFT'
  }
  for directory in ['/emsdk' '/emsdk/upstream' '/emsdk/upstream/bin'] { require ((ls -lD $directory | first).type == 'dir') 'TOOL_ANCESTOR_DRIFT' }
  let clang_sha = (inspect_clang '/emsdk/upstream/bin/clang' '/emsdk/upstream/bin')
  let emcc_sha = (digest '/emsdk/upstream/emscripten/emcc')
  let llvm_ar_sha = (digest '/emsdk/upstream/bin/llvm-ar')
  require ($emcc_sha == $ledger.expected_tool_sha256.emcc and $clang_sha == $ledger.expected_tool_sha256.clang and $llvm_ar_sha == $ledger.expected_tool_sha256.llvm_ar) 'TOOL_DRIFT'
  let image = (build_condition image '/work/image/cache' $output $ledger)
  let clean = (build_condition clean '/work/clean/cache' $output $ledger)
  mkdir $"($output)/sources"
  for row in $ledger.sources {
    let retained = $"($output)/sources/($row.member | str replace -r '\.o$' '.c')"
    cp $row.path $retained
    require ((digest $retained) == $row.sha256) 'SOURCE_COPY_DRIFT'
  }
  cp $ledger.emscripten_notice_path $"($output)/emscripten-LICENSE"
  cp $ledger.musl_notice_path $"($output)/musl-COPYRIGHT"
  require ((digest $"($output)/emscripten-LICENSE") == $ledger.emscripten_notice_sha256 and (digest $"($output)/musl-COPYRIGHT") == $ledger.musl_notice_sha256) 'NOTICE_COPY_DRIFT'
  {schema: $ledger.schema, status: 'PRODUCED_DIAGNOSTIC_ONLY', image_digest: $ledger.image,
   source_commit: $ledger.official_source_commit, recipe_sha256: $ledger.recipe_sha256,
   embuilder_sha256: $ledger.embuilder_sha256, selected_license: $ledger.selected_license,
   tool_sha256: {emcc: $emcc_sha, clang: $clang_sha, llvm_ar: $llvm_ar_sha},
   old_producer_run: $ledger.old_producer_run, old_producer_bill_sha256: $ledger.old_producer_bill_sha256,
   emscripten_notice_sha256: $ledger.emscripten_notice_sha256, musl_notice_sha256: $ledger.musl_notice_sha256,
   conditions: [$image $clean], object_input_closure: 'UNPROVEN', sdk_archive_keys_pending_per_condition: 9,
   generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', installed_libsockets_attribution: 'UNPROVEN', glue_legal_admission: 'NO',
   legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $"($output)/controlled-object.json"
  rm $"($output)/refusal.json"
}
