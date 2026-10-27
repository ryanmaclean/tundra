#!/usr/bin/env nu

# Replay byte parity only. Trace shape is diagnostic and cannot prove input lineage.
# No raw trace or absolute path is a receipt.
def refusal [receipt: string, reason: string, details: record = {}] {
  let base = {
    schema: 'tundra-library-sigs-lineage-diagnostic/v1'
    status: 'REFUSED'
    reason: $reason
    generator_source_mode: 'NOT_ESTABLISHED'
    input_manifest: 'UNPROVEN'
    generator_input_closure: 'UNPROVEN'
    selected_graph_exhaustive: 'NO'
    glue_legal_admission: 'NO'
    legal_admission: 'NO'
    product_admission: 'NO'
    fleet_admission: 'NO'
  }
  ($base | merge $details | to json --raw) | save --force $"($receipt)/refusal.json"
  error make {msg: $reason}
}

def file_sha [path: string] {
  let out = (^sha256sum -- $path | complete)
  if $out.exit_code != 0 { error make {msg: 'HASH_FAILED'} }
  ($out.stdout | str substring 0..63)
}

export def verify_file [path: string, expected_size: int, expected_sha: string] {
  if not ($path | path exists) { error make {msg: 'FILE_MISSING'} }
  let item = (ls -l $path | first)
  if $item.type != 'file' { error make {msg: 'FILE_DRIFT'} }
  let size = ($item.size | into int)
  if $size != $expected_size or (file_sha $path) != $expected_sha {
    error make {msg: 'FILE_DRIFT'}
  }
}

export def verify_required_file [stage: string, path: string, expected_size: int, expected_sha: string] {
  let stages = ['PINNED_GENERATOR' 'TRACKED_OUTPUT' 'GENERATED_OUTPUT' 'EH_IMAGE_COPY' 'EH_CLEAN_COPY' 'SHARED_MODULE' 'UTILS_MODULE' 'WEBASSEMBLY_MODULE' 'COMPILER_JS']
  if $stage not-in $stages { error make {msg: 'REQUIRED_STAGE_UNCLASSIFIED'} }
  try {
    verify_file $path $expected_size $expected_sha
  } catch {|e|
    let reason = ($e.msg | default 'UNKNOWN')
    if $reason in ['FILE_MISSING' 'FILE_DRIFT' 'HASH_FAILED'] {
      error make {msg: $"($stage)_($reason)"}
    }
    error make {msg: 'REQUIRED_FILE_UNCLASSIFIED'}
  }
}

export def classify_container_start_failure [stderr: string] {
  let target = '/emsdk/upstream/emscripten/tools/maint/gen_sig_info.py'
  if ($stderr | str contains $target) and (
      ($stderr | str contains 'invalid mount config') or
      ($stderr | str contains 'error mounting') or
      ($stderr | str contains 'mount through procfd')) {
    'GENERATOR_BIND_UNAVAILABLE'
  } else {
    'ISOLATED_GENERATOR_REFUSED'
  }
}

export def exit_value_bucket [exit_code: int] {
  if $exit_code == 124 {
    'EXIT_124_TIMEOUT_COMPATIBLE'
  } else if $exit_code == 125 {
    'EXIT_125'
  } else if $exit_code in [126 127] {
    'EXIT_126_OR_127'
  } else if $exit_code >= 128 and $exit_code <= 255 {
    'EXIT_128_TO_255'
  } else if $exit_code >= 1 and $exit_code <= 123 {
    'EXIT_1_TO_123'
  } else {
    'EXIT_OUT_OF_RANGE'
  }
}

export def signed_invocation_status [exit_code: int] {
  if $exit_code == -11 { 'NEGATIVE_SIGSEGV_COMPATIBLE' } else if $exit_code == -6 { 'NEGATIVE_SIGABRT_COMPATIBLE' } else if $exit_code == -9 { 'NEGATIVE_SIGKILL_COMPATIBLE' } else if $exit_code == -4 { 'NEGATIVE_SIGILL_COMPATIBLE' } else if $exit_code == -15 { 'NEGATIVE_SIGTERM_COMPATIBLE' } else if $exit_code < 0 and $exit_code >= -64 { 'NEGATIVE_OTHER_SIGNAL_COMPATIBLE' } else if $exit_code < -64 { 'NEGATIVE_OUT_OF_SIGNAL_RANGE' } else if $exit_code <= 255 { 'NONNEGATIVE_EXIT_VALUE' } else { 'NONNEGATIVE_OUT_OF_RANGE' }
}

export def classify_python_execve_line [line: string] {
  let call = ($line | str replace -r '^\s*(\[pid\s+\d+\]\s*|\d+\s+)?execve\(' '')
  if $call == $line { return 'UNPROVEN' }
  let chars = ($call | split chars | enumerate)
  let state = ($chars | reduce --fold {depth: 1, quoted: false, escaped: false, closed_at: -1} {|ch, acc|
    if $acc.closed_at >= 0 { $acc } else if $acc.escaped { $acc | update escaped false } else if $acc.quoted and $ch.item == '\' { $acc | update escaped true } else if $ch.item == '"' { $acc | update quoted (not $acc.quoted) } else if not $acc.quoted and $ch.item == '(' { $acc | update depth ($acc.depth + 1) } else if not $acc.quoted and $ch.item == ')' { if $acc.depth == 1 { {depth: 0, quoted: false, escaped: false, closed_at: $ch.index} } else { $acc | update depth ($acc.depth - 1) } } else { $acc }
  })
  if $state.closed_at < 0 or $state.quoted or $state.escaped { return 'UNPROVEN' }
  let suffix = ($chars | skip ($state.closed_at + 1) | each {|ch| $ch.item} | str join '')
  if $suffix =~ '^\s+=\s+0\s*$' { 'ZERO_TEXT' } else if $suffix =~ '^\s+=\s+-1\s+[A-Z][A-Z0-9_]*(\s+\([^()\n]*\))?\s*$' { 'NONZERO_TEXT' } else { 'UNPROVEN' }
}

export def classify_python_execve_text [lines: list] {
  let selected = ($lines | where {|line| $line =~ '^\s*(\[pid\s+\d+\]\s*|\d+\s+)?execve\("[^"\n]*/python3",'})
  if ($selected | length) == 0 { return 'PYTHON_EXECVE_NOT_SEEN' }
  if ($selected | length) > 64 { return 'PYTHON_EXECVE_COUNT_CAP' }
  let classes = ($selected | each {|line| classify_python_execve_line $line})
  if ('UNPROVEN' in $classes) { return 'PYTHON_EXECVE_RETURN_UNPROVEN' }
  if ('ZERO_TEXT' in $classes) { 'PYTHON_EXECVE_ZERO_TEXT_PRESENT' } else { 'PYTHON_EXECVE_NONZERO_TEXT_ONLY' }
}

export def classify_failed_invocation [exit_code: int, trace_path: string, trace_cap: int] {
  let exit_class = (exit_value_bucket $exit_code)
  mut python_stage = 'PYTHON_EXECVE_UNPROVEN'
  let trace_stage = if not ($trace_path | path exists) {
    'TRACE_MISSING'
  } else {
    let item = (ls -l $trace_path | first)
    if $item.type != 'file' {
      'TRACE_NONREGULAR'
    } else {
      let size = ($item.size | into int)
      if $size == 0 {
        'TRACE_EMPTY'
      } else if $size > $trace_cap {
        'TRACE_OVER_CAP'
      } else {
        let lines = (open --raw $trace_path | lines)
        if ($lines | length) > 200000 {
          'TRACE_OVER_LINE_CAP'
        } else {
          $python_stage = (classify_python_execve_text $lines)
          if ($lines | any {|line| $line =~ 'execve\('}) { 'TRACE_BOUNDED_EXECVE_TEXT' } else { 'TRACE_BOUNDED_NO_EXECVE_TEXT' }
        }
      }
    }
  }
  {invocation_exit_class: $exit_class, invocation_signed_status: (signed_invocation_status $exit_code), invocation_trace_stage: $trace_stage, invocation_python_execve: $python_stage}
}

export def classify_direct_output [path: string, expected_size: int, expected_sha: string, copies: list<string>] {
  let verified = (try { verify_file $path $expected_size $expected_sha; true } catch { false })
  if not $verified { return 'DIRECT_OUTPUT_MISSING_OR_DRIFT' }
  if ($copies | length) != 3 { return 'DIRECT_CHECK_REFUSED' }
  for copy in $copies {
    let comparison = (try { ^cmp -s -- $path $copy | complete } catch { return 'DIRECT_CHECK_REFUSED' })
    if $comparison.exit_code == 1 { return 'DIRECT_OUTPUT_DIFFERENT' }
    if $comparison.exit_code != 0 { return 'DIRECT_CHECK_REFUSED' }
  }
  'DIRECT_BYTE_EQUAL'
}

export def paired_control_class [direct_status: string, trace_status: string] {
  let direct_equal = $direct_status == 'DIRECT_BYTE_EQUAL'
  let trace_equal = $trace_status == 'TRACE_BYTE_EQUAL'
  if $direct_equal and $trace_equal { 'PAIRED_BOTH_PARITY' } else if $direct_equal { 'PAIRED_DIRECT_ONLY_PARITY' } else if $trace_equal { 'PAIRED_TRACE_ONLY_PARITY' } else { 'PAIRED_NO_PARITY_PROVEN' }
}

export def classify_tracer_loader_list [exit_code: int, stdout: string, stderr: string] {
  if ($stdout | str length) > 8192 or ($stderr | str length) > 8192 { return 'TRACER_LOADER_LOG_CAP' }
  if $exit_code != 0 { return 'TRACER_LOADER_LIST_REFUSED' }
  if ($stdout | str contains 'not found') or ($stderr | str contains 'not found') {
    return 'TRACER_LOADER_LIST_DRIFT'
  }
  let rows = ($stdout | lines)
  for soname in [
    'libunwind-ptrace\.so\.0'
    'libunwind-x86_64\.so\.8'
    'libunwind\.so\.8'
    'liblzma\.so\.5'
    'libc\.so\.6'
  ] {
    let selected = ($rows | where {|line| $line =~ ('^\s*' + $soname + '\s+=>')})
    let pinned = ('^\s*' + $soname + '\s+=>\s+/tool/tracer-runtime/' + $soname + '\s+\(')
    if ($selected | length) != 1 or not (($selected | first) =~ $pinned) { return 'TRACER_LOADER_LIST_DRIFT' }
  }
  'PASS'
}

export def classify_tracer_loader_failure [exit_code: int, stdout: string, stderr: string] {
  if ($stdout | str length) > 8192 or ($stderr | str length) > 8192 {
    error make {msg: 'TRACER_LOADER_LOG_CAP'}
  }
  let rows = ($stdout | lines)
  let known = [
    {name: 'LIBUNWIND_PTRACE', pattern: 'libunwind-ptrace\.so\.0'}
    {name: 'LIBUNWIND_X86_64', pattern: 'libunwind-x86_64\.so\.8'}
    {name: 'LIBUNWIND', pattern: 'libunwind\.so\.8'}
    {name: 'LIBLZMA', pattern: 'liblzma\.so\.5'}
    {name: 'LIBC', pattern: 'libc\.so\.6'}
    {name: 'LOADER', pattern: 'ld-linux-x86-64\.so\.2'}
  ]
  mut missing = []
  for item in $known {
    let row_pattern = ('^\s*' + $item.pattern + '\s+=>\s+not found\s*$')
    let error_pattern = ('(^|\s)' + $item.pattern + ': cannot open shared object file')
    if (($rows | any {|line| $line =~ $row_pattern}) or ($stderr =~ $error_pattern)) {
      $missing = ($missing | append $item.name)
    }
  }
  let report_class = if ($missing | length) > 0 {
    'KNOWN_SONAME_NOT_FOUND_TEXT'
  } else if $stderr =~ 'GLIBC_[0-9]+\.[0-9]+.*not found' {
    'GLIBC_VERSION_NOT_FOUND_TEXT'
  } else if ($stderr | str contains 'wrong ELF class') {
    'ELF_CLASS_MISMATCH_TEXT'
  } else if ($stdout | str contains 'not found') or ($stderr | str contains 'not found') {
    'OTHER_NOT_FOUND_TEXT'
  } else {
    'NO_RECOGNIZED_LOADER_TEXT'
  }
  {loader_exit_class: (exit_value_bucket $exit_code), loader_report_class: $report_class, loader_missing_sonames: $missing}
}

export def classify_tracer_version [exit_code: int, stdout: string, stderr: string] {
  if ($stdout | str length) > 8192 or ($stderr | str length) > 8192 { return 'TRACER_VERSION_LOG_CAP' }
  if $exit_code != 0 { return 'TRACER_VERSION_REFUSED' }
  if (($stdout | lines | first) != 'strace -- version 5.16') { return 'TRACER_VERSION_DRIFT' }
  'PASS'
}

export def trace_shape [path: string, cap: int] {
  if not ($path | path exists) { error make {msg: 'TRACE_MISSING'} }
  let item = (ls -l $path | first)
  if $item.type != 'file' { error make {msg: 'TRACE_CAP_OR_EMPTY'} }
  let size = ($item.size | into int)
  if $size == 0 or $size > $cap { error make {msg: 'TRACE_CAP_OR_EMPTY'} }
  let lines = (open --raw $path | lines)
  if ($lines | length) > 200000 { error make {msg: 'TRACE_LINE_CAP'} }
  let execs = ($lines | where $it =~ 'execve\(' | length)
  let opens = ($lines | where $it =~ '(open|openat|openat2)\(' | length)
  if $execs < 3 or $opens < 1 { error make {msg: 'EXEC_OR_OPEN_WITNESS_MISSING'} }
  # The parser cannot safely infer a different process's cwd or a reused dirfd.
  # Refuse these shapes rather than assign an input to the wrong source tree.
  let relative = ($lines | where {|line| (($line =~ 'openat\(') and ($line !~ 'openat\(AT_FDCWD, "/')) or (($line =~ 'openat2\(') and ($line !~ 'openat2\(AT_FDCWD, "/')) or (($line =~ 'open\(') and ($line !~ 'open\("/')) or (($line =~ 'execve\(') and ($line !~ 'execve\("/')) })
  let dirfd = ($lines | where $it =~ 'openat(2)?\([0-9]+')
  let unfinished = ($lines | where $it =~ '<unfinished \.\.\.>|resumed>')
  let unknown = ($lines | where $it =~ '(io_uring|open_by_handle_at|name_to_handle_at|fchdir|chdir)\(')
  let network = ($lines | where $it =~ '(socket|connect|sendto|recvfrom|sendmsg|recvmsg)\(')
  {trace_bytes: $size, trace_lines: ($lines | length), exec_lines: $execs, open_lines: $opens, relative_shape_lines: ($relative | length), dirfd_shape_lines: ($dirfd | length), split_shape_lines: ($unfinished | length), other_path_mechanism_lines: ($unknown | length), network_shape_lines: ($network | length)}
}

export def verify_repo_digests [tagged_ref: string, repo_digests_json: string] {
  let expected_tagged = 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29'
  let expected_repo_digest = 'emscripten/emsdk@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29'
  if $tagged_ref != $expected_tagged { error make {msg: 'IMAGE_DIGEST_DRIFT'} }
  let digests = ($repo_digests_json | from json)
  if (($digests | describe) != 'list<string>') or (($digests | length) < 1) or (($digests | length) > 16) {
    error make {msg: 'IMAGE_DIGEST_DRIFT'}
  }
  if (($digests | uniq | length) != ($digests | length)) or (not ($expected_repo_digest in $digests)) {
    error make {msg: 'IMAGE_DIGEST_DRIFT'}
  }
  $expected_repo_digest
}

def inside [ledger_path: string, receipt: string] {
  let ledger = (open $ledger_path)
  let source = '/emsdk/upstream/emscripten'
  try {
    verify_required_file PINNED_GENERATOR $"($source)/($ledger.generator_path)" 12317 $ledger.generator_sha256
    verify_required_file SHARED_MODULE $"($source)/tools/shared.py" 27249 'a0d5e0d92e49e062bd083782f2569b6ff6eb8328ddf60d50d28d86ec1dee6840'
    verify_required_file UTILS_MODULE $"($source)/tools/utils.py" 2761 'd786b040de3197fd8949bc407ef502f28dade58f7ffd1289197db8ff3bcf176f'
    verify_required_file WEBASSEMBLY_MODULE $"($source)/tools/webassembly.py" 15298 '23eea83f1745c959b7d88f09f77488249cc686e56a5de46c929617083acca3c8'
    verify_required_file COMPILER_JS $"($source)/src/compiler.js" 3793 '2a49cf913f39fb2795458ed6cfba56942576d511b57b4e290e1c667a24cfdf30'
    verify_required_file TRACKED_OUTPUT $"($source)/($ledger.output_path)" $ledger.output_size $ledger.output_sha256
    if (file_sha $"($source)/($ledger.notice_path)") != $ledger.notice_sha256 {
      error make {msg: 'NOTICE_DRIFT'}
    }
    if $ledger.image != 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29' {
      error make {msg: 'IMAGE_DRIFT'}
    }
    let pre = {generator: (file_sha $"($source)/($ledger.generator_path)"), tracked_output: (file_sha $"($source)/($ledger.output_path)"), notice: (file_sha $"($source)/($ledger.notice_path)")}
    let runtime = '/tool/tracer-runtime'
    try {
      verify_file $"($runtime)/libunwind-ptrace.so.0" 18840 'f791a9b0b01faa13a65ee066d2351d2e38a23d6c3752f75eebeaf0fc5c7d56f4'
      verify_file $"($runtime)/libunwind-x86_64.so.8" 71920 '1077e265bdd76ed4f2d628ea29150eb98f5c7c8f5a30004a561e6e93a2a6f097'
      verify_file $"($runtime)/libunwind.so.8" 63744 '5d1f6f104f79698e137c9f17c4b0317dfd96816c0eb6a87c50619a41472ca382'
      verify_file $"($runtime)/liblzma.so.5" 170456 '493cb401ab4aa3bba611ca464d12996afb3b327940d29476f535f999e167439b'
      verify_file $"($runtime)/libc.so.6" 2216304 '2e15e345ccba1e1e8bf3bc58938d13eb7ccc17044942f0376cd5bc771f429b79'
      verify_file $"($runtime)/ld-linux-x86-64.so.2" 240936 'e833bd99168bea158d2f2e5d32462479cacfb624003304a9ea74a1607a144f1f'
    } catch { error make {msg: 'TRACER_RUNTIME_DRIFT'} }
    let interpreter = $"($runtime)/ld-linux-x86-64.so.2"
    let interpreter_check = (try { ^test -f $interpreter -a -x $interpreter | complete } catch { error make {msg: 'TRACER_INTERPRETER_UNAVAILABLE'} })
    if $interpreter_check.exit_code != 0 { error make {msg: 'TRACER_INTERPRETER_UNAVAILABLE'} }
    let loader_list = (try { ^timeout 8s $interpreter --library-path $runtime --list /tool/strace | complete } catch { error make {msg: 'TRACER_LOADER_LIST_REFUSED'} })
    let loader_class = (classify_tracer_loader_list $loader_list.exit_code $loader_list.stdout $loader_list.stderr)
    if $loader_class == 'TRACER_LOADER_LIST_REFUSED' {
      refusal $receipt $loader_class (classify_tracer_loader_failure $loader_list.exit_code $loader_list.stdout $loader_list.stderr)
    }
    if $loader_class != 'PASS' { error make {msg: $loader_class} }
    let tracer_version = (try { ^timeout 8s $interpreter --library-path $runtime /tool/strace -V | complete } catch { error make {msg: 'TRACER_VERSION_REFUSED'} })
    let version_class = (classify_tracer_version $tracer_version.exit_code $tracer_version.stdout $tracer_version.stderr)
    if $version_class != 'PASS' { error make {msg: $version_class} }
    verify_required_file EH_IMAGE_COPY /tool/image-library_sigs.js $ledger.output_size $ledger.output_sha256
    verify_required_file EH_CLEAN_COPY /tool/clean-library_sigs.js $ledger.output_size $ledger.output_sha256
    let copies = [$"($source)/($ledger.output_path)" /tool/image-library_sigs.js /tool/clean-library_sigs.js]
    mkdir /work/direct /work/traced /out/direct /out/traced
    mkdir /tmp/direct-home /tmp/direct-emcache /tmp/direct-tmp /tmp/traced-home /tmp/traced-emcache /tmp/traced-tmp
    cd /work/direct
    let direct = (with-env {HOME: '/tmp/direct-home', EM_CACHE: '/tmp/direct-emcache', TMPDIR: '/tmp/direct-tmp'} {
      ^timeout 600s python3 $"($source)/($ledger.generator_path)" -o /out/direct/library_sigs.js | complete
    })
    let direct_status = if ($direct.stdout | str length) > 1048576 or ($direct.stderr | str length) > 1048576 {
      'DIRECT_LOG_CAP'
    } else if $direct.exit_code != 0 {
      'DIRECT_EXIT_REFUSED'
    } else {
      classify_direct_output /out/direct/library_sigs.js $ledger.output_size $ledger.output_sha256 $copies
    }
    cd /work
    rm -rf /work/direct /out/direct /tmp/direct-home /tmp/direct-emcache /tmp/direct-tmp
    cd /work/traced
    let invocation = (with-env {HOME: '/tmp/traced-home', EM_CACHE: '/tmp/traced-emcache', TMPDIR: '/tmp/traced-tmp'} {
      ^timeout 600s $interpreter --library-path $runtime /tool/strace -f -yy -s 4096 -o /out/traced/trace.raw -e 'trace=%file,%process,%network,read,readv,close,chdir,fchdir' python3 $"($source)/($ledger.generator_path)" -o /out/traced/library_sigs.js | complete
    })
    mut trace_status = 'TRACE_SHAPE_OR_OUTPUT_REFUSED'
    mut trace_details = {}
    if ($invocation.stdout | str length) > 1048576 or ($invocation.stderr | str length) > 1048576 {
      $trace_status = 'TRACE_LOG_CAP'
    } else if $invocation.exit_code != 0 {
      $trace_status = 'TRACE_INVOCATION_REFUSED'
      $trace_details = (classify_failed_invocation $invocation.exit_code /out/traced/trace.raw $ledger.trace_cap_bytes)
    } else {
      $trace_status = (try {
        trace_shape /out/traced/trace.raw $ledger.trace_cap_bytes | ignore
        classify_direct_output /out/traced/library_sigs.js $ledger.output_size $ledger.output_sha256 $copies
      } catch { 'DIRECT_CHECK_REFUSED' })
      $trace_status = if $trace_status == 'DIRECT_BYTE_EQUAL' { 'TRACE_BYTE_EQUAL' } else if $trace_status == 'DIRECT_OUTPUT_DIFFERENT' { 'TRACE_OUTPUT_DIFFERENT' } else { 'TRACE_SHAPE_OR_OUTPUT_REFUSED' }
    }
    let post = {generator: (file_sha $"($source)/($ledger.generator_path)"), tracked_output: (file_sha $"($source)/($ledger.output_path)"), notice: (file_sha $"($source)/($ledger.notice_path)")}
    if $pre != $post { error make {msg: 'PINNED_INPUT_CHANGED'} }
    refusal $receipt 'PAIRED_GENERATOR_CONTROL' ($trace_details | merge {
      direct_control: $direct_status
      traced_control: $trace_status
      paired_control: (paired_control_class $direct_status $trace_status)
      tracer_link_smoke: 'PASS'
      tracer_version_smoke: 'PASS'
    })
  } catch {|e|
    # Preserve already-written bounded loader or invocation diagnostic classes.
    let existing = (open $"($receipt)/refusal.json")
    let message = ($e.msg | default 'UNKNOWN')
    if $message in ['GENERATOR_OR_TRACER_FAILED' 'TRACER_LOADER_LIST_REFUSED' 'PAIRED_GENERATOR_CONTROL'] and $existing.reason == $message {
      error make {msg: $message}
    }
    let allowed = [
      'FILE_MISSING' 'FILE_DRIFT' 'HASH_FAILED' 'NOTICE_DRIFT' 'IMAGE_DRIFT'
      'GENERATOR_OR_TRACER_FAILED' 'TRACE_MISSING' 'TRACE_CAP_OR_EMPTY'
      'TRACE_LINE_CAP' 'EXEC_OR_OPEN_WITNESS_MISSING' 'RELATIVE_PATH_UNRESOLVED'
      'DIRFD_UNRESOLVED' 'SPLIT_SYSCALL_UNRESOLVED'
      'OTHER_PATH_MECHANISM_UNRESOLVED' 'OUTPUT_BYTES_DIFFER'
      'PINNED_INPUT_CHANGED' 'GENERATOR_LOG_CAP'
      'PINNED_GENERATOR_FILE_MISSING' 'PINNED_GENERATOR_FILE_DRIFT' 'PINNED_GENERATOR_HASH_FAILED'
      'TRACKED_OUTPUT_FILE_MISSING' 'TRACKED_OUTPUT_FILE_DRIFT' 'TRACKED_OUTPUT_HASH_FAILED'
      'GENERATED_OUTPUT_FILE_MISSING' 'GENERATED_OUTPUT_FILE_DRIFT' 'GENERATED_OUTPUT_HASH_FAILED'
      'EH_IMAGE_COPY_FILE_MISSING' 'EH_IMAGE_COPY_FILE_DRIFT' 'EH_IMAGE_COPY_HASH_FAILED'
      'EH_CLEAN_COPY_FILE_MISSING' 'EH_CLEAN_COPY_FILE_DRIFT' 'EH_CLEAN_COPY_HASH_FAILED'
      'SHARED_MODULE_FILE_MISSING' 'SHARED_MODULE_FILE_DRIFT' 'SHARED_MODULE_HASH_FAILED'
      'UTILS_MODULE_FILE_MISSING' 'UTILS_MODULE_FILE_DRIFT' 'UTILS_MODULE_HASH_FAILED'
      'WEBASSEMBLY_MODULE_FILE_MISSING' 'WEBASSEMBLY_MODULE_FILE_DRIFT' 'WEBASSEMBLY_MODULE_HASH_FAILED'
      'COMPILER_JS_FILE_MISSING' 'COMPILER_JS_FILE_DRIFT' 'COMPILER_JS_HASH_FAILED'
      'TRACER_INTERPRETER_UNAVAILABLE' 'TRACER_LOADER_LOG_CAP' 'TRACER_LOADER_LIST_REFUSED'
      'TRACER_RUNTIME_DRIFT' 'TRACER_LOADER_LIST_DRIFT' 'TRACER_VERSION_LOG_CAP' 'TRACER_VERSION_REFUSED'
      'TRACER_VERSION_DRIFT'
    ]
    if $message in $allowed { refusal $receipt $message } else { refusal $receipt 'UNCLASSIFIED_FAILURE' }
  }
}

def host [ledger_path: string, input: string, private: string, receipt: string] {
  let ledger = (open $ledger_path)
  if $ledger.generator_input_closure != 'UNPROVEN' or $ledger.glue_legal_admission != 'NO' or $ledger.legal_admission != 'NO' or $ledger.product_admission != 'NO' or $ledger.fleet_admission != 'NO' {
    refusal $receipt 'ADMISSION_DRIFT'
  }
  verify_file $"($input)/strace" 1972848 '38a5c75cb29dd85ddd7780d54f5bf595554d7a1b5c42524b23065f5dc4c4b01d'
  try {
    verify_file $"($input)/gen_sig_info.py" 12317 $ledger.generator_sha256
  } catch {
    refusal $receipt 'GENERATOR_SOURCE_DRIFT'
  }
  let tool = $"($input)/tool"
  mkdir $tool
  cp -- $"($input)/strace" $"($tool)/strace"
  cp -- $"($input)/image-library_sigs.js" $"($tool)/image-library_sigs.js"
  cp -- $"($input)/clean-library_sigs.js" $"($tool)/clean-library_sigs.js"
  cp -- $ledger_path $"($tool)/ledger.json"
  cp -- .github/scripts/duckdb-library-sigs-lineage-v1.nu $"($tool)/helper.nu"
  cp -- $"($env.RUNNER_TEMP)/tundra-nu-0.115.1/nu" $"($tool)/nu"
  # Stage only the previously hashed package bytes inside the existing read-only /tool bind.
  ^cp -R -- $"($input)/tracer-runtime" $"($tool)/tracer-runtime"
  ^chmod -R a+rX -- $tool
  ^chmod 0777 -- $private
  ^chmod 0777 -- $receipt
  ^chmod 0666 -- $"($receipt)/refusal.json"
  let pulled = (^timeout 900s docker pull $ledger.image | complete)
  if $pulled.exit_code != 0 { refusal $receipt 'PINNED_IMAGE_UNAVAILABLE' }
  let inspected = (^docker image inspect --format '{{json .RepoDigests}}' $ledger.image | complete)
  if $inspected.exit_code != 0 { refusal $receipt 'IMAGE_DIGEST_DRIFT' }
  try {
    verify_repo_digests $ledger.image ($inspected.stdout | str trim) | ignore
  } catch {
    refusal $receipt 'IMAGE_DIGEST_DRIFT'
  }
  let result = (^docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --pids-limit 128 --memory 4g --cpus 2 --ulimit fsize=16777216:16777216 --user 1000:1000 --env HOME=/tmp --env EM_CACHE=/tmp/emcache --env TUNDRA_SIGS_INSIDE=1 --tmpfs /work:rw,nosuid,nodev,size=67108864 --tmpfs /tmp:rw,nosuid,nodev,size=536870912 --mount $"type=bind,src=($tool),dst=/tool,readonly" --mount $"type=bind,src=($input)/gen_sig_info.py,dst=/emsdk/upstream/emscripten/tools/maint/gen_sig_info.py,readonly" --mount $"type=bind,src=($private),dst=/out" --mount $"type=bind,src=($receipt),dst=/receipt" --entrypoint /tool/nu $ledger.image /tool/helper.nu /tool/ledger.json /tool /out /receipt | complete)
  if $result.exit_code != 0 {
    if (open $"($receipt)/refusal.json").reason == 'NOT_COMPLETED' {
      refusal $receipt (classify_container_start_failure $result.stderr)
    }
    error make {msg: 'ISOLATED_GENERATOR_REFUSED'}
  }
  if not ($"($receipt)/replay.json" | path exists) or ($"($receipt)/refusal.json" | path exists) {
    refusal $receipt 'REPLAY_RECEIPT_MISSING'
  }
}

def main [ledger_path: string, input: string, private: string, receipt: string] {
  if 'TUNDRA_SIGS_INSIDE' in $env {
    inside $ledger_path $receipt
  } else {
    host $ledger_path $input $private $receipt
  }
}
