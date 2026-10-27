#!/usr/bin/env nu

# Replay byte parity only. Trace shape is diagnostic and cannot prove input lineage.
# No raw trace or absolute path is a receipt.
def refusal [receipt: string, reason: string, details: record = {}] {
  let base = {
    schema: 'tundra-library-sigs-lineage-diagnostic/v1'
    status: 'REFUSED'
    reason: $reason
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
    verify_file $"($source)/($ledger.generator_path)" 12317 $ledger.generator_sha256
    verify_file $"($source)/($ledger.output_path)" $ledger.output_size $ledger.output_sha256
    if (file_sha $"($source)/($ledger.notice_path)") != $ledger.notice_sha256 {
      error make {msg: 'NOTICE_DRIFT'}
    }
    if $ledger.image != 'emscripten/emsdk:3.1.48@sha256:c952f592577a428698cb990c9656f395ae34424740f9c40bed4c3ee6d4850d29' {
      error make {msg: 'IMAGE_DRIFT'}
    }
    let pre = {generator: (file_sha $"($source)/($ledger.generator_path)"), tracked_output: (file_sha $"($source)/($ledger.output_path)"), notice: (file_sha $"($source)/($ledger.notice_path)")}
    cd /work
    let invocation = (^timeout 600s /tool/strace -f -yy -s 4096 -o /out/trace.raw -e 'trace=%file,%process,%network,read,readv,close,chdir,fchdir' python3 $"($source)/($ledger.generator_path)" -o /out/library_sigs.js | complete)
    if $invocation.exit_code != 0 { error make {msg: 'GENERATOR_OR_TRACER_FAILED'} }
    if ($invocation.stdout | str length) > 1048576 or ($invocation.stderr | str length) > 1048576 {
      error make {msg: 'GENERATOR_LOG_CAP'}
    }
    let shape = (trace_shape /out/trace.raw $ledger.trace_cap_bytes)
    verify_file /out/library_sigs.js $ledger.output_size $ledger.output_sha256
    verify_file /tool/image-library_sigs.js $ledger.output_size $ledger.output_sha256
    verify_file /tool/clean-library_sigs.js $ledger.output_size $ledger.output_sha256
    let parity = ((^cmp -s /out/library_sigs.js $"($source)/($ledger.output_path)" | complete).exit_code == 0)
      and ((^cmp -s /out/library_sigs.js /tool/image-library_sigs.js | complete).exit_code == 0)
      and ((^cmp -s /out/library_sigs.js /tool/clean-library_sigs.js | complete).exit_code == 0)
    if not $parity { error make {msg: 'OUTPUT_BYTES_DIFFER'} }
    let post = {generator: (file_sha $"($source)/($ledger.generator_path)"), tracked_output: (file_sha $"($source)/($ledger.output_path)"), notice: (file_sha $"($source)/($ledger.notice_path)")}
    if $pre != $post { error make {msg: 'PINNED_INPUT_CHANGED'} }
    let result = {
      schema: 'tundra-library-sigs-lineage-diagnostic/v1'
      status: 'REPLAY_BYTE_EQUAL'
      source_scope: 'PINNED_IMAGE_AND_TWO_AUTHENTICATED_EH_COPIES'
      image: $ledger.image
      invocation: 'python3 tools/maint/gen_sig_info.py -o scratch/library_sigs.js'
      cwd_role: 'ISOLATED_SCRATCH'
      home_role: 'ISOLATED_TMPFS'
      em_cache_role: 'ISOLATED_TMPFS'
      network: 'NONE'
      source_root: 'READ_ONLY_IMAGE'
      output_sha256: $ledger.output_sha256
      output_size: $ledger.output_size
      generator_script_sha256: $ledger.generator_sha256
      generator_script_notice_status: 'PINNED_HEADER_MIT_OR_NCSA_SELECTED_MIT'
      tracked_notice_status: $ledger.source_status
      other_inputs_notice_status: 'UNPROVEN'
      known_input_pre_sha256: $pre
      known_input_post_sha256: $post
      input_manifest: 'UNPROVEN'
      generator_input_closure: 'UNPROVEN'
      selected_graph_exhaustive: 'NO'
      glue_legal_admission: 'NO'
      legal_admission: 'NO'
      product_admission: 'NO'
      fleet_admission: 'NO'
      trace_shape: $shape
    }
    ($result | to json --raw) | save --force $"($receipt)/replay.json"
    rm $"($receipt)/refusal.json"
  } catch {|e|
    let allowed = [
      'FILE_MISSING' 'FILE_DRIFT' 'HASH_FAILED' 'NOTICE_DRIFT' 'IMAGE_DRIFT'
      'GENERATOR_OR_TRACER_FAILED' 'TRACE_MISSING' 'TRACE_CAP_OR_EMPTY'
      'TRACE_LINE_CAP' 'EXEC_OR_OPEN_WITNESS_MISSING' 'RELATIVE_PATH_UNRESOLVED'
      'DIRFD_UNRESOLVED' 'SPLIT_SYSCALL_UNRESOLVED'
      'OTHER_PATH_MECHANISM_UNRESOLVED' 'OUTPUT_BYTES_DIFFER'
      'PINNED_INPUT_CHANGED' 'GENERATOR_LOG_CAP'
    ]
    let message = ($e.msg | default 'UNKNOWN')
    if $message in $allowed { refusal $receipt $message } else { refusal $receipt 'UNCLASSIFIED_FAILURE' }
  }
}

def host [ledger_path: string, input: string, private: string, receipt: string] {
  let ledger = (open $ledger_path)
  if $ledger.generator_input_closure != 'UNPROVEN' or $ledger.glue_legal_admission != 'NO' or $ledger.legal_admission != 'NO' or $ledger.product_admission != 'NO' or $ledger.fleet_admission != 'NO' {
    refusal $receipt 'ADMISSION_DRIFT'
  }
  verify_file $"($input)/strace" 1972848 '38a5c75cb29dd85ddd7780d54f5bf595554d7a1b5c42524b23065f5dc4c4b01d'
  let tool = $"($input)/tool"
  mkdir $tool
  cp -- $"($input)/strace" $"($tool)/strace"
  cp -- $"($input)/image-library_sigs.js" $"($tool)/image-library_sigs.js"
  cp -- $"($input)/clean-library_sigs.js" $"($tool)/clean-library_sigs.js"
  cp -- $ledger_path $"($tool)/ledger.json"
  cp -- .github/scripts/duckdb-library-sigs-lineage-v1.nu $"($tool)/helper.nu"
  cp -- $"($env.RUNNER_TEMP)/tundra-nu-0.115.1/nu" $"($tool)/nu"
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
  let result = (^docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --pids-limit 128 --memory 4g --cpus 2 --ulimit fsize=16777216:16777216 --user 1000:1000 --env HOME=/tmp --env EM_CACHE=/tmp/emcache --env TUNDRA_SIGS_INSIDE=1 --tmpfs /work:rw,nosuid,nodev,size=67108864 --tmpfs /tmp:rw,nosuid,nodev,size=536870912 --mount $"type=bind,src=($tool),dst=/tool,readonly" --mount $"type=bind,src=($private),dst=/out" --mount $"type=bind,src=($receipt),dst=/receipt" --entrypoint /tool/nu $ledger.image /tool/helper.nu /tool/ledger.json /tool /out /receipt | complete)
  if $result.exit_code != 0 {
    if (open $"($receipt)/refusal.json").reason == 'NOT_COMPLETED' { refusal $receipt 'ISOLATED_GENERATOR_REFUSED' }
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
