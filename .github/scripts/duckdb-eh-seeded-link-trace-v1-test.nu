#!/usr/bin/env nu

use ./duckdb-eh-seeded-link-trace-v1.nu [canary_shape canary_diagnostic relink_diagnostic]
use ./duckdb-eh-linker-fd-witness-v1.nu [witness parser_failure_class ellipsis_class]
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def main [] {
  require ((parser_failure_class 'OPEN_SHAPE_AMBIGUOUS') == 'OPEN_SHAPE_AMBIGUOUS') 'PARSER_FIXED_CLASS'
  require ((parser_failure_class 'REFUSED_DIRECT_READ_NOT_PROVED') == 'REFUSED_DIRECT_READ_NOT_PROVED') 'PARSER_MISSING_READ_CLASS'
  require ((parser_failure_class 'private/path: OPEN_SHAPE_AMBIGUOUS') == 'PARSER_OTHER') 'PARSER_PRIVATE_ERROR_LEAK'
  require ((parser_failure_class 'UNEXPECTED_NU_ERROR') == 'PARSER_OTHER') 'PARSER_UNLISTED_CLASS'
  let root = ($env.FILE_PWD | path join '../fixtures/duckdb-eh-seeded-link-trace-v1')
  let sentinel = '/work/image/trace-private/canary.txt'
  let positive = (open --raw ($root | path join 'canary-positive.trace'))
  require (canary_shape $positive $sentinel) 'POSITIVE_CANARY'
  require (canary_shape ($positive | str replace 'O_RDONLY|O_CLOEXEC' 'O_RDONLY') $sentinel) 'PLAIN_OS_OPEN_CANARY'
  let direct_open = 'openat(AT_FDCWD, "/work/image/trace-private/canary.txt", O_RDONLY|O_CLOEXEC) = 3</work/image/trace-private/canary.txt>'
  let duplicate = ($positive | str replace $direct_open $"($direct_open)\n($direct_open)")
  let duplicate_refused = (try {
    canary_shape $duplicate $sentinel | ignore
    false
  } catch {|err| $err.msg == 'CANARY_DIRECT_OPEN' })
  require $duplicate_refused 'DUPLICATE_OPEN_ACCEPTED'
  let diagnostic = (canary_diagnostic $duplicate $sentinel 'CANARY_DIRECT_OPEN')
  require ($diagnostic.failure == 'CANARY_DIRECT_OPEN' and $diagnostic.sentinel_open_lines == 2 and $diagnostic.plain_at_fdcwd_open_lines == 2 and $diagnostic.plain_sentinel_open_lines == 2 and $diagnostic.sentinel_quoted_argument_lines == 2 and $diagnostic.sentinel_numeric_result_lines == 2 and $diagnostic.sentinel_fd_annotation_lines == 2 and $diagnostic.exact_path_opens == 2 and $diagnostic.readonly_path_opens == 2 and $diagnostic.numeric_fd_opens == 2 and $diagnostic.matching_fd_annotation_opens == 2 and $diagnostic.exact_sentinel_opens == 2 and $diagnostic.read_lines == 1) 'DUPLICATE_DIAGNOSTIC'
  require (not (($diagnostic | to json --raw) | str contains $sentinel)) 'DIAGNOSTIC_PATH_LEAK'
  let other_annotation = ($positive | str replace '3</work/image/trace-private/canary.txt>' '3</another-file>' )
  let annotation = (canary_diagnostic $other_annotation $sentinel 'CANARY_DIRECT_OPEN')
  require ($annotation.sentinel_open_lines == 1 and $annotation.exact_path_opens == 1 and $annotation.readonly_path_opens == 1 and $annotation.numeric_fd_opens == 1 and $annotation.matching_fd_annotation_opens == 0 and $annotation.exact_sentinel_opens == 0) 'ANNOTATION_STAGE'
  let no_annotation = ($positive | str replace '3</work/image/trace-private/canary.txt>' '3')
  let no_fd_annotation = (canary_diagnostic $no_annotation $sentinel 'CANARY_DIRECT_OPEN')
  require ($no_fd_annotation.exact_path_opens == 1 and $no_fd_annotation.numeric_fd_opens == 1 and $no_fd_annotation.matching_fd_annotation_opens == 0 and $no_fd_annotation.exact_sentinel_opens == 0) 'MISSING_ANNOTATION_STAGE'
  let other_path = ($positive | str replace '"/work/image/trace-private/canary.txt"' '"/work/image/trace-private/canary.txt.bak"')
  let path_stage = (canary_diagnostic $other_path $sentinel 'CANARY_DIRECT_OPEN')
  require ($path_stage.sentinel_open_lines == 1 and $path_stage.sentinel_quoted_path_lines == 0 and $path_stage.sentinel_fd_annotation_lines == 1 and $path_stage.exact_path_opens == 0) 'PATH_STAGE'
  let other_flag = ($positive | str replace 'O_RDONLY|O_CLOEXEC' 'O_WRONLY')
  let flag_stage = (canary_diagnostic $other_flag $sentinel 'CANARY_DIRECT_OPEN')
  require ($flag_stage.exact_path_opens == 1 and $flag_stage.readonly_path_opens == 0) 'FLAG_STAGE'
  let annotated_fdcwd = ($positive | str replace 'openat(AT_FDCWD, "' 'openat(AT_FDCWD</work/image>, "')
  require (canary_shape $annotated_fdcwd $sentinel) 'ANNOTATED_FDCWD_CANARY'
  let annotated_stage = (canary_diagnostic $annotated_fdcwd $sentinel 'CANARY_DIRECT_OPEN')
  require ($annotated_stage.openat_lines == 1 and $annotated_stage.plain_at_fdcwd_open_lines == 0 and $annotated_stage.annotated_at_fdcwd_open_lines == 1 and $annotated_stage.plain_sentinel_open_lines == 0 and $annotated_stage.annotated_sentinel_open_lines == 1 and $annotated_stage.sentinel_quoted_path_lines == 1 and $annotated_stage.sentinel_quoted_argument_lines == 1 and $annotated_stage.sentinel_result_marker_lines == 1 and $annotated_stage.sentinel_numeric_result_lines == 1 and $annotated_stage.sentinel_fd_annotation_lines == 1 and $annotated_stage.parsed_at_fdcwd_opens == 1 and $annotated_stage.exact_sentinel_opens == 1) 'ANNOTATED_FDCWD_STAGE'
  let malformed_dirfd = ($annotated_fdcwd | str replace 'AT_FDCWD</work/image>' 'AT_FDCWD<work/image>')
  let malformed_dirfd_refused = (try { canary_shape $malformed_dirfd $sentinel | ignore; false } catch {|err| $err.msg == 'CANARY_DIRECT_OPEN' })
  require $malformed_dirfd_refused 'RELATIVE_DIRFD_ACCEPTED'
  let wrong_fd_annotation = ($annotated_fdcwd | str replace '3</work/image/trace-private/canary.txt>' '3</another-file>')
  let wrong_fd_refused = (try { canary_shape $wrong_fd_annotation $sentinel | ignore; false } catch {|err| $err.msg == 'CANARY_DIRECT_OPEN' })
  require $wrong_fd_refused 'WRONG_FD_ANNOTATION_ACCEPTED'
  let annotated_zero_read = ((open --raw ($root | path join 'canary-zero-read.trace')) | str replace 'openat(AT_FDCWD, "' 'openat(AT_FDCWD</work/image>, "')
  let annotated_zero_refused = (try { canary_shape $annotated_zero_read $sentinel | ignore; false } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $annotated_zero_refused 'ANNOTATED_ZERO_READ_ACCEPTED'
  let other_read_fd = ($annotated_fdcwd | str replace 'read(3<' 'read(4<')
  let other_read_fd_refused = (try { canary_shape $other_read_fd $sentinel | ignore; false } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $other_read_fd_refused 'OTHER_READ_FD_ACCEPTED'
  let missing_exit = ($annotated_fdcwd | str replace '+++ exited with 0 +++' '+++ exited with 1 +++')
  let missing_exit_refused = (try { canary_shape $missing_exit $sentinel | ignore; false } catch {|err| $err.msg == 'CANARY_CHILD_EXIT' })
  require $missing_exit_refused 'NONZERO_CHILD_EXIT_ACCEPTED'
  let unfinished_open = ($positive | str replace 'O_RDONLY|O_CLOEXEC) = 3</work/image/trace-private/canary.txt>' 'O_RDONLY|O_CLOEXEC <unfinished ...>')
  let unfinished_stage = (canary_diagnostic $unfinished_open $sentinel 'CANARY_DIRECT_OPEN')
  require ($unfinished_stage.sentinel_open_lines == 1 and $unfinished_stage.sentinel_quoted_argument_lines == 1 and $unfinished_stage.sentinel_unfinished_lines == 1 and $unfinished_stage.sentinel_result_marker_lines == 0 and $unfinished_stage.sentinel_numeric_result_lines == 0 and $unfinished_stage.sentinel_fd_annotation_lines == 0 and $unfinished_stage.parsed_at_fdcwd_opens == 0) 'UNFINISHED_OPEN_STAGE'
  require (not (($annotated_stage | to json --raw) | str contains $sentinel)) 'ANNOTATED_DIAGNOSTIC_PATH_LEAK'
  let refused = (try {
    canary_shape (open --raw ($root | path join 'canary-zero-read.trace')) $sentinel | ignore
    false
  } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $refused 'ZERO_READ_ACCEPTED'
  let scratch = (^mktemp -d | str trim)
  let linker = '/emsdk/upstream/bin/wasm-ld'
  let archive = '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a'
  let trace_dir = ($scratch | path join 'traces')
  mkdir $trace_dir
  let trace = ($trace_dir | path join 'trace.123')
  let ledger = ($env.FILE_PWD | path join '../receipts/duckdb-eh-linker-fd-witness-v1.json')
  let parser_receipt = ($scratch | path join 'parser.json')
  let linker_hash = '6e5c12aab29b80db118acd16c08953257683ed11c415970f8320ed777997eec7'
  let exec_line = ('execve("LINKER_PATH", ["wasm-ld"], 0x1234) = 0' | str replace 'LINKER_PATH' $linker)
  let open_line = ('openat(AT_FDCWD, "ARCHIVE_PATH", O_RDONLY) = 3<ARCHIVE_PATH>' | str replace --all 'ARCHIVE_PATH' $archive)
  let exit_line = '+++ exited with 0 +++'
  let base = [$exec_line $open_line]
  let cases = [
    {input: ($base | append ['read(3, "PRIVATE_SECRET...", 20) = 20' $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|READ|LINKER_ACTIVE|ACTIVE'},
    {input: ($base | append ['pread64(4, "PRIVATE_SECRET...", 20, 0) = 20' $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|PREAD64|LINKER_ACTIVE|OTHER'},
    {input: ($base | append [('mmap(NULL, 100, PROT_READ, MAP_PRIVATE, 3<ARCHIVE_PATH>, ...) = 0x123' | str replace 'ARCHIVE_PATH' $archive) $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|MMAP|LINKER_ACTIVE|ACTIVE'},
    {input: ($base | append ['mmap(NULL, 100, PROT_READ, MAP_PRIVATE, ..., 0) = 0x123' $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|MMAP|LINKER_ACTIVE|UNKNOWN'},
    {input: ($base | append [('dup(3<ARCHIVE_PATH>) = 4<ARCHIVE_PATH>' | str replace --all 'ARCHIVE_PATH' $archive) 'read(4, "PRIVATE_SECRET...", 20) = 20' $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|READ|LINKER_ACTIVE|ACTIVE'},
    {input: ($base | append [('close(3<ARCHIVE_PATH>) = 0' | str replace 'ARCHIVE_PATH' $archive) 'read(3, "PRIVATE_SECRET...", 20) = 20' $exit_line]), expected: 'TRUNCATED_OR_AMBIGUOUS|READ|LINKER_NO_ACTIVE|OTHER'},
    {input: [('execve("LINKER_PATH", ["PRIVATE_SECRET", ...], 0x1234) = 0' | str replace 'LINKER_PATH' $linker)], expected: 'TRUNCATED_OR_AMBIGUOUS|EXECVE|CANDIDATE_EXEC|UNKNOWN'},
    {input: ['openat(AT_FDCWD, "PRIVATE_SECRET...", O_RDONLY) = 4</private/path>'], expected: 'TRUNCATED_OR_AMBIGUOUS|OPENAT|BEFORE_LINKER|UNKNOWN'}
  ]
  for case in $cases {
    $case.input | str join (char newline) | save --force $trace
    let failure = (try { witness $ledger $trace_dir $linker_hash $parser_receipt; 'ACCEPTED' } catch {|err| parser_failure_class $err.msg })
    require ($failure == $case.expected) 'ELLIPSIS_REFUSAL_CLASS'
    require (not ($failure | str contains 'PRIVATE_SECRET') and not ($failure | str contains $archive) and not ($failure | str contains $scratch)) 'ELLIPSIS_PRIVATE_LEAK'
  }
  let unknown = (ellipsis_class 'read(999999999999999999999, "PRIVATE_SECRET...", 1) = 1' true false [3] $linker)
  require ($unknown == 'TRUNCATED_OR_AMBIGUOUS|READ|LINKER_ACTIVE|UNKNOWN') 'ELLIPSIS_FD_UNKNOWN'
  require ((parser_failure_class 'TRUNCATED_OR_AMBIGUOUS|READ|LINKER_ACTIVE|PRIVATE_SECRET') == 'PARSER_OTHER') 'ELLIPSIS_ALLOWLIST'
  # A second task may share and mutate the FD table before the archive open.
  # The single PID trace alone must not accept that archive/read sequence.
  for creation in [
    'clone(child_stack=NULL, flags=CLONE_VM|CLONE_FILES|SIGCHLD) = 124'
    'clone3({flags=CLONE_FILES}, 88) = 124'
    'fork() = 124'
    'vfork() = 124'
  ] {
    [$exec_line $creation $open_line 'read(3, 0x1234, 10) = 10' $exit_line] | str join (char newline) | save --force $trace
    let failure = (try { witness $ledger $trace_dir $linker_hash $parser_receipt; 'ACCEPTED' } catch {|err| parser_failure_class $err.msg })
    require ($failure == 'CHILD_CREATION_AMBIGUOUS') 'SHARED_FD_CREATION_ACCEPTED'
  }
  # Exercise descriptor lifetime, including replacement by dup2, rather than
  # treating the matching descriptor number as permanent after an open.
  let close_archive = ('close(3<ARCHIVE_PATH>) = 0' | str replace 'ARCHIVE_PATH' $archive)
  let dup_archive = ('dup(3<ARCHIVE_PATH>) = 4<ARCHIVE_PATH>' | str replace --all 'ARCHIVE_PATH' $archive)
  let replace_archive = ('dup2(4</fixture/other>, 3<ARCHIVE_PATH>) = 3</fixture/other>' | str replace 'ARCHIVE_PATH' $archive)
  let fd_cases = [
    {events: [$close_archive 'read(3, 0x1234, 10) = 10'], expected: 'REFUSED_DIRECT_READ_NOT_PROVED'},
    {events: ['close_range(3, 3, 0) = 0' 'read(3, 0x1234, 10) = 10'], expected: 'REFUSED_DIRECT_READ_NOT_PROVED'},
    {events: [$replace_archive 'read(3, 0x1234, 10) = 10'], expected: 'REFUSED_DIRECT_READ_NOT_PROVED'},
    {events: ['fcntl(3, F_DUPFD, 4) = 4' 'read(3, 0x1234, 10) = 10'], expected: 'FCNTL_AMBIGUOUS'},
    {events: ['read(3</fixture/other>, 0x1234, 10) = 10'], expected: 'FD_ANNOTATION_DRIFT'}
  ]
  for case in $fd_cases {
    $base | append $case.events | append $exit_line | str join (char newline) | save --force $trace
    let failure = (try { witness $ledger $trace_dir $linker_hash $parser_receipt; 'ACCEPTED' } catch {|err| parser_failure_class $err.msg })
    require ($failure == $case.expected) 'FD_MUTATION_CASE'
    rm --force $parser_receipt
  }
  $base | append [$dup_archive $close_archive 'read(4, 0x1234, 10) = 10' $exit_line] | str join (char newline) | save --force $trace
  witness $ledger $trace_dir $linker_hash $parser_receipt
  require ((open $parser_receipt).status == 'FIXTURE_DIRECT_READ_SHAPE') 'DUPLICATED_FD_READ_REFUSED'
  rm $parser_receipt
  $base | append ['read(3, 0x1234, 10) = 10' $exit_line] | str join (char newline) | save --force $trace
  witness $ledger $trace_dir $linker_hash $parser_receipt
  require ((open $parser_receipt).status == 'FIXTURE_DIRECT_READ_SHAPE') 'POSITIVE_READ_WITHOUT_CLOSE_REFUSED'
  rm $parser_receipt
  $base | append ['read(3, 0x1234, 10) = 0' $exit_line] | str join (char newline) | save --force $trace
  let zero_failure = (try { witness $ledger $trace_dir $linker_hash $parser_receipt; 'ACCEPTED' } catch {|err| parser_failure_class $err.msg })
  require ($zero_failure == 'REFUSED_DIRECT_READ_NOT_PROVED') 'ZERO_READ_PARSER_ACCEPTED'
  rm $parser_receipt
  $base | append ['read(3, 0x1234, 10 <unfinished ...>'] | str join (char newline) | save --force $trace
  let split_failure = (try { witness $ledger $trace_dir $linker_hash $parser_receipt; 'ACCEPTED' } catch {|err| parser_failure_class $err.msg })
  require ($split_failure == 'SPLIT_SYSCALL_AMBIGUOUS') 'SPLIT_SYSCALL_ACCEPTED'
  let receipt = ($scratch | path join 'refusal.json')
  for classification in [TIMEOUT EXIT_NONZERO TRACE_CAP OUTPUT_ABSENT] {
    {schema: 'tundra-eh-seeded-link-trace/v1', status: 'REFUSED', reason: 'TRACE_RELINK', legal_admission: 'NO'} | to json --raw | save --force $receipt
    relink_diagnostic $classification '129' '16777217' 'no' 'yes' 'no' $receipt
    let observed = (open $receipt)
    require ($observed.reason == 'TRACE_RELINK' and $observed.legal_admission == 'NO' and $observed.relink_diagnostic.classification == $classification and $observed.relink_diagnostic.trace_file_count_capped == 129 and $observed.relink_diagnostic.trace_bytes_capped == 16777217 and $observed.relink_diagnostic.trace_file_cap_exceeded and $observed.relink_diagnostic.trace_bytes_cap_exceeded and not $observed.relink_diagnostic.fresh_map_present and $observed.relink_diagnostic.fresh_js_present and not $observed.relink_diagnostic.fresh_wasm_present) 'RELINK_DIAGNOSTIC_SHAPE'
    require (not (($observed | to json --raw) | str contains $scratch)) 'RELINK_DIAGNOSTIC_PATH_LEAK'
  }
  relink_diagnostic 'TRACE_CAP' '128' '16777216' 'no' 'no' 'no' $receipt
  let within = (open $receipt)
  require (not $within.relink_diagnostic.trace_file_cap_exceeded and not $within.relink_diagnostic.trace_bytes_cap_exceeded) 'RELINK_BOUNDARY_REFUSED'
  let unbounded = (try { relink_diagnostic 'TIMEOUT' '130' '0' 'no' 'no' 'no' $receipt; false } catch {|err| $err.msg == 'RELINK_COUNT_CAP' })
  require $unbounded 'RELINK_UNBOUNDED_COUNT_ACCEPTED'
  let invalid = (try { relink_diagnostic 'RAW_TRACE' '1' '1' 'no' 'no' 'no' $receipt; false } catch {|err| $err.msg == 'RELINK_CLASSIFICATION' })
  require $invalid 'RELINK_UNBOUNDED_CLASSIFICATION_ACCEPTED'
  rm -r $scratch
  print 'trace canary positive and zero-read refusal: PASS'
}
