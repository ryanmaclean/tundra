#!/usr/bin/env nu

use ./duckdb-eh-seeded-link-trace-v1.nu [canary_shape canary_diagnostic]
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def main [] {
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
  require ($diagnostic.failure == 'CANARY_DIRECT_OPEN' and $diagnostic.sentinel_open_lines == 2 and $diagnostic.exact_path_opens == 2 and $diagnostic.readonly_path_opens == 2 and $diagnostic.numeric_fd_opens == 2 and $diagnostic.matching_fd_annotation_opens == 2 and $diagnostic.exact_sentinel_opens == 2 and $diagnostic.read_lines == 1) 'DUPLICATE_DIAGNOSTIC'
  require (not (($diagnostic | to json --raw) | str contains $sentinel)) 'DIAGNOSTIC_PATH_LEAK'
  let other_annotation = ($positive | str replace '3</work/image/trace-private/canary.txt>' '3</another-file>' )
  let annotation = (canary_diagnostic $other_annotation $sentinel 'CANARY_DIRECT_OPEN')
  require ($annotation.sentinel_open_lines == 1 and $annotation.exact_path_opens == 1 and $annotation.readonly_path_opens == 1 and $annotation.numeric_fd_opens == 1 and $annotation.matching_fd_annotation_opens == 0 and $annotation.exact_sentinel_opens == 0) 'ANNOTATION_STAGE'
  let no_annotation = ($positive | str replace '3</work/image/trace-private/canary.txt>' '3')
  let no_fd_annotation = (canary_diagnostic $no_annotation $sentinel 'CANARY_DIRECT_OPEN')
  require ($no_fd_annotation.exact_path_opens == 1 and $no_fd_annotation.numeric_fd_opens == 1 and $no_fd_annotation.matching_fd_annotation_opens == 0 and $no_fd_annotation.exact_sentinel_opens == 0) 'MISSING_ANNOTATION_STAGE'
  let other_path = ($positive | str replace '"/work/image/trace-private/canary.txt"' '"/work/image/trace-private/canary.txt.bak"')
  let path_stage = (canary_diagnostic $other_path $sentinel 'CANARY_DIRECT_OPEN')
  require ($path_stage.sentinel_open_lines == 1 and $path_stage.exact_path_opens == 0) 'PATH_STAGE'
  let other_flag = ($positive | str replace 'O_RDONLY|O_CLOEXEC' 'O_WRONLY')
  let flag_stage = (canary_diagnostic $other_flag $sentinel 'CANARY_DIRECT_OPEN')
  require ($flag_stage.exact_path_opens == 1 and $flag_stage.readonly_path_opens == 0) 'FLAG_STAGE'
  let refused = (try {
    canary_shape (open --raw ($root | path join 'canary-zero-read.trace')) $sentinel | ignore
    false
  } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $refused 'ZERO_READ_ACCEPTED'
  print 'trace canary positive and zero-read refusal: PASS'
}
