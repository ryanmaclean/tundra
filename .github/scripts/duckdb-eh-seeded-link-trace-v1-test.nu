#!/usr/bin/env nu

use ./duckdb-eh-seeded-link-trace-v1.nu canary_shape
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def main [] {
  let root = ($env.FILE_PWD | path join '../fixtures/duckdb-eh-seeded-link-trace-v1')
  let sentinel = '/work/image/trace-private/canary.txt'
  require (canary_shape (open --raw ($root | path join 'canary-positive.trace')) $sentinel) 'POSITIVE_CANARY'
  let refused = (try {
    canary_shape (open --raw ($root | path join 'canary-zero-read.trace')) $sentinel | ignore
    false
  } catch {|err| $err.msg == 'CANARY_DIRECT_READ' })
  require $refused 'ZERO_READ_ACCEPTED'
  print 'trace canary positive and zero-read refusal: PASS'
}
