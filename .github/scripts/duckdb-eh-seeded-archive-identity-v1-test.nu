#!/usr/bin/env nu

# No private artifact is needed: execute dispatch/refusal and member selection.
use ./duckdb-eh-seeded-archive-identity-v1.nu selected_source

def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }

def main [nu_bin: string] {
  let helper = ($env.FILE_PWD | path join 'duckdb-eh-seeded-archive-identity-v1.nu')
  let invalid = (^$nu_bin $helper bogus x y z w v | complete)
  require ($invalid.exit_code != 0 and ($invalid.stderr | str contains 'x MODE') and not ($invalid.stderr | str contains 'Command `else` not found')) 'INVALID_MODE_REFUSAL'

  let short_seed = (^$nu_bin $helper seed x y z w v | complete)
  require ($short_seed.exit_code != 0 and ($short_seed.stderr | str contains 'x SEED_ARGUMENTS') and not ($short_seed.stderr | str contains 'Command `else` not found')) 'SEED_ARGUMENT_REFUSAL'

  require ((selected_source '/work/image/pr143-auth' 'image-libnoexit.a') == '/work/image/pr143-auth/image-libnoexit.a') 'IMAGE_SELECTION'
  let clean_refused = (try {
    selected_source '/work/image/pr143-auth' 'clean-libnoexit.a' | ignore
    false
  } catch {|err| $err.msg == 'CONDITION_SOURCE_NOT_IMAGE' })
  require $clean_refused 'CLEAN_MEMBER_REFUSAL'
  print 'seed dispatch and condition-source refusals: PASS'
}
