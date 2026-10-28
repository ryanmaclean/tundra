#!/usr/bin/env nu

use ./duckdb-eh-seeded-link-trace-v1.nu [canary_shape canary_diagnostic relink_diagnostic]
use ./duckdb-eh-linker-fd-witness-v1.nu parser_failure_class
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
