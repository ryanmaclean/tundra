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
  require ($diagnostic.failure == 'CANARY_DIRECT_OPEN' and $diagnostic.sentinel_open_lines == 2 and $diagnostic.exact_sentinel_opens == 2 and $diagnostic.read_lines == 1) 'DUPLICATE_DIAGNOSTIC'
  require (not (($diagnostic | to json --raw) | str contains $sentinel)) 'DIAGNOSTIC_PATH_LEAK'
  let refused = (try {
    canary_shape (open --raw ($root | path join 'canary-zero-read.trace')) $sentinel | ignore
    false
  } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $refused 'ZERO_READ_ACCEPTED'
  print 'trace canary positive and zero-read refusal: PASS'
}
