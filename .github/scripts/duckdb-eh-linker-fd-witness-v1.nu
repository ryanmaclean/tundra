#!/usr/bin/env nu

# Strict -ff/-yy fixture parser. This does not authenticate or run an EH link.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def capture [line: string, pattern: string] {
  let rows = ($line | parse -r $pattern)
  if ($rows | is-empty) { null } else { $rows | first }
}
def regular_size [path: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') 'TRACE_FILE_NOT_REGULAR'
  ($rows | first).size | into int
}
def checked_ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-eh-linker-fd-witness/v1' and $l.scope == 'STANDALONE_TRACE_FIXTURE_PARSER_ONLY' and $l.condition == 'image') 'LEDGER_SCOPE'
  require ($l.preflight_run_id == 40549577194 and $l.preflight_head == '53d6f5af4d0ebbad18c24672f777ebb70a69417d') 'LEDGER_PREFLIGHT'
  require ($l.configured_linker_path == '/emsdk/upstream/bin/wasm-ld' and $l.resolved_linker_path == '/emsdk/upstream/bin/lld' and $l.linker_elf_sha256 == '6e5c12aab29b80db118acd16c08953257683ed11c415970f8320ed777997eec7') 'LEDGER_LINKER'
  require ($l.producer_run_id == 40536996581 and $l.producer_artifact_id == 12748008032 and $l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.archive_path == '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a') 'LEDGER_ARCHIVE'
  require ($l.max_trace_files == 128 and $l.max_trace_file_bytes == 4194304 and $l.max_total_trace_bytes == 16777216 and $l.max_events == 50000 and $l.max_active_fds == 256 and $l.max_line_bytes == 8192) 'LEDGER_CAP'
  require ($l.actual_eh_linker_child == 'UNPROVEN' and $l.linker_archive_read == 'UNPROVEN' and $l.same_run_eh_outputs == 'UNPROVEN' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_ADMISSION'
  $l
}

def parse_pid [raw: string, pid: int, l: record] {
  mut exec_ok = false
  mut exited_before_candidate = false
  mut exited = false
  mut active = []
  mut read_positive = false
  mut mapped = false
  mut opens = 0
  mut reads = 0
  mut maps = 0
  mut events = 0
  for line in ($raw | lines) {
    $events += 1
    require ($events <= $l.max_events and ($line | str length) <= $l.max_line_bytes) 'TRACE_EVENT_CAP'
    require (not ($line | str contains '<unfinished ...>') and not ($line | str contains 'resumed>')) 'SPLIT_SYSCALL_AMBIGUOUS'
    require (not ($line | str contains '...') or ($line | str starts-with '+++ exited with ')) 'TRUNCATED_OR_AMBIGUOUS'
    if ($line | str starts-with 'execve(') {
      let e = (capture $line '^execve\("(?<path>[^"\\]+)", .*\) = (?<result>-?[0-9]+)$')
      require ($e != null) 'EXECVE_SHAPE'
      if $e.path == $l.configured_linker_path {
        require (not $exec_ok and not $exited_before_candidate and $e.result == '0') 'LINKER_EXECVE_NOT_UNIQUE_SUCCESS'
        $exec_ok = true
      } else if $exec_ok { error make {msg: 'LINKER_PID_REEXEC'} }
      continue
    }
    if not $exec_ok {
      if ($line | str starts-with '+++ exited with ') { $exited_before_candidate = true }
      continue
    }
    require (not $exited) 'EVENT_AFTER_EXIT'
    if $line == '+++ exited with 0 +++' { $exited = true; continue }
    if ($line | str starts-with '+++ exited') or ($line | str starts-with '--- ') { error make {msg: 'LINKER_EXIT_NOT_ZERO'} }
    if ($line | str starts-with 'chdir(') or ($line | str starts-with 'fchdir(') { error make {msg: 'CWD_CHANGE_AMBIGUOUS'} }
    if ($line | str starts-with 'execveat(') { error make {msg: 'EXECVEAT_AMBIGUOUS'} }
    if ($line | str starts-with 'open(') or ($line | str starts-with 'openat(') {
      let o = if ($line | str starts-with 'openat(') {
        capture $line '^openat\((?<dirfd>[^,]+), "(?<path>[^"\\]+)", (?<flags>[^)]*)\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$'
      } else {
        capture $line '^open\("(?<path>[^"\\]+)", (?<flags>[^)]*)\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$'
      }
      require ($o != null) 'OPEN_SHAPE_AMBIGUOUS'
      require ($o.path | str starts-with '/') 'RELATIVE_OPEN_AMBIGUOUS'
      if 'dirfd' in $o { require ($o.dirfd == 'AT_FDCWD') 'OPENAT_DIRFD_AMBIGUOUS' }
      let fd = ($o.fd | into int)
      $active = ($active | where $it != $fd)
      if $o.path == $l.archive_path {
        require ($o.annotation == $l.archive_path and ($o.flags | str starts-with 'O_RDONLY') and not ($o.flags | str contains 'O_WRONLY') and not ($o.flags | str contains 'O_RDWR')) 'ARCHIVE_OPEN_NOT_READONLY_EXACT'
        $active = ($active | append $fd)
        $opens += 1
      }
      require (($active | length) <= $l.max_active_fds) 'FD_CAP'
      continue
    }
    if ($line | str starts-with 'close(') {
      let c = (capture $line '^close\((?<fd>[0-9]+)(?:<(?<annotation>[^>]+)>)?\) = (?<result>-?[0-9]+)$')
      require ($c != null) 'CLOSE_SHAPE_AMBIGUOUS'
      if (($c.fd | into int) in $active) { require ($c.annotation == $l.archive_path) 'FD_ANNOTATION_DRIFT' }
      if $c.result == '0' { $active = ($active | where $it != ($c.fd | into int)) }
      continue
    }
    if ($line | str starts-with 'close_range(') {
      let c = (capture $line '^close_range\((?<lo>[0-9]+), (?<hi>[0-9]+), (?<flags>[0-9]+)\) = (?<result>-?[0-9]+)$')
      require ($c != null) 'CLOSE_RANGE_AMBIGUOUS'
      if $c.result == '0' { $active = ($active | where $it < ($c.lo | into int) or $it > ($c.hi | into int)) }
      continue
    }
    if ($line | str starts-with 'dup(') or ($line | str starts-with 'dup2(') or ($line | str starts-with 'dup3(') {
      let d = if ($line | str starts-with 'dup(') {
        capture $line '^dup\((?<old>[0-9]+)<(?<old_path>[^>]+)>\) = (?<new>[0-9]+)<(?<new_path>[^>]+)>$'
      } else if ($line | str starts-with 'dup2(') {
        capture $line '^dup2\((?<old>[0-9]+)<(?<old_path>[^>]+)>, (?<requested>[0-9]+)(?:<[^>]+>)?\) = (?<new>[0-9]+)<(?<new_path>[^>]+)>$'
      } else {
        capture $line '^dup3\((?<old>[0-9]+)<(?<old_path>[^>]+)>, (?<requested>[0-9]+)(?:<[^>]+>)?, [A-Z0-9_|]+\) = (?<new>[0-9]+)<(?<new_path>[^>]+)>$'
      }
      require ($d != null) 'DUP_SHAPE_AMBIGUOUS'
      let old = ($d.old | into int)
      let new = ($d.new | into int)
      if 'requested' in $d { require ($new == ($d.requested | into int)) 'DUP_RETURN_DRIFT' }
      let inherited = ($old in $active)
      if $inherited { require ($d.old_path == $l.archive_path and $d.new_path == $l.archive_path) 'FD_ANNOTATION_DRIFT' }
      $active = ($active | where $it != $new)
      if $inherited { $active = ($active | append $new | uniq) }
      require (($active | length) <= $l.max_active_fds) 'FD_CAP'
      continue
    }
    if ($line | str starts-with 'fcntl(') { error make {msg: 'FCNTL_AMBIGUOUS'} }
    if ($line | str starts-with 'read(') or ($line | str starts-with 'pread64(') or ($line | str starts-with 'readv(') {
      # A raw-pointer read may omit -yy's FD annotation. The same PID's
      # exact-path open and active-FD lifetime still bind its positive result.
      let r = (capture $line '^(?:read|pread64|readv)\((?<fd>[0-9]+)(?:<(?<annotation>[^>]+)>)?, .*\) = (?<result>-?[0-9]+)$')
      require ($r != null) 'READ_SHAPE_AMBIGUOUS'
      if (($r.fd | into int) in $active) {
        require ($r.annotation == null or $r.annotation == '' or $r.annotation == $l.archive_path) 'FD_ANNOTATION_DRIFT'
        if ($r.result | into int) > 0 { $read_positive = true; $reads += 1 }
      }
      continue
    }
    if ($line | str starts-with 'mmap(') {
      let m = (capture $line '^mmap\([^,]+, (?<length>[0-9]+), (?<prot>[^,]+), (?<flags>[^,]+), (?<fd>[0-9]+)<(?<annotation>[^>]+)>, [^)]*\) = (?<result>0x[0-9a-f]+)$')
      require ($m != null) 'MMAP_SHAPE_AMBIGUOUS'
      if (($m.fd | into int) in $active) {
        require ($m.annotation == $l.archive_path) 'FD_ANNOTATION_DRIFT'
        if ($m.prot | str contains 'PROT_READ') and ($m.flags | str contains 'MAP_PRIVATE') and (($m.length | into int) > 0) { $mapped = true; $maps += 1 }
      }
      continue
    }
    if ($line | str starts-with 'exit_group(') {
      require ($line == 'exit_group(0) = ?') 'LINKER_EXIT_NOT_ZERO'
      continue
    }
    if ($line | str starts-with 'munmap(') { continue }
    if ($active | length) > 0 { error make {msg: 'UNRECOGNIZED_ACTIVE_FD_EVENT'} }
  }
  {pid: $pid, exec_ok: $exec_ok, exited: $exited, direct_archive_opens: $opens, positive_reads: $reads, mappings: $maps, read_positive: $read_positive, mapped: $mapped, events: $events}
}

export def witness [ledger_path: string, trace_dir: string, runtime_elf_sha256: string, receipt_path: string] {
  let l = (checked_ledger $ledger_path)
  require ($runtime_elf_sha256 == $l.linker_elf_sha256) 'RUNTIME_ELF_HASH_DRIFT'
  let dir_rows = (try { ls -lD $trace_dir } catch { [] })
  require (($dir_rows | length) == 1 and ($dir_rows | first).type == 'dir') 'TRACE_DIR'
  let entries = (ls -l $trace_dir)
  require (($entries | length) > 0 and ($entries | all {|e| ($e.name | path basename) =~ '^trace\.[0-9]+$' and $e.type == 'file'})) 'TRACE_FILE_SCOPE'
  let files = ($entries | sort-by name)
  require (($files | length) > 0 and ($files | length) <= $l.max_trace_files) 'TRACE_FILE_CAP'
  let names = ($files | get name | each {|p| $p | path basename })
  require (($names | uniq | length) == ($names | length)) 'TRACE_PID_DUPLICATE'
  mut total = 0
  mut total_events = 0
  mut candidates = []
  for file in $files {
    let size = (regular_size $file.name)
    $total += $size
    require ($size > 0 and $size <= $l.max_trace_file_bytes and $total <= $l.max_total_trace_bytes) 'TRACE_BYTE_CAP'
    let pid = (($file.name | path basename | str replace 'trace.' '') | into int)
    let result = (parse_pid (open --raw $file.name) $pid $l)
    $total_events += $result.events
    require ($total_events <= $l.max_events) 'TRACE_EVENT_CAP'
    if $result.exec_ok { $candidates = ($candidates | append $result) }
  }
  require (($candidates | length) == 1) 'LINKER_PID_NOT_UNIQUE'
  let c = ($candidates | first)
  require $c.exited 'LINKER_EXIT_NOT_PROVED'
  let status = if $c.read_positive { 'FIXTURE_DIRECT_READ_SHAPE' } else if $c.mapped { 'REFUSED_MAPPED_ONLY' } else { 'REFUSED_DIRECT_READ_NOT_PROVED' }
  {schema: $l.schema, status: $status, linker_pid: $c.pid, preflight_run_id: $l.preflight_run_id, producer_run_id: $l.producer_run_id,
   configured_linker_path: $l.configured_linker_path, linker_elf_sha256: $l.linker_elf_sha256,
   archive_sha256: $l.archive_sha256, direct_archive_opens: $c.direct_archive_opens,
   positive_reads: $c.positive_reads, mappings: $c.mappings, trace_file_count: ($files | length),
   trace_bytes: $total, actual_eh_linker_child: 'UNPROVEN', linker_archive_read: 'UNPROVEN',
   same_run_eh_outputs: 'UNPROVEN', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt_path
  require $c.read_positive $status
}

export def parser_failure_class [message: string] {
  # Only literal parser assertions may cross into a fixed refusal. Nu errors
  # can contain private paths or trace content, so all others collapse.
  let allowed = [
    ARCHIVE_OPEN_NOT_READONLY_EXACT CLOSE_RANGE_AMBIGUOUS CLOSE_SHAPE_AMBIGUOUS
    CWD_CHANGE_AMBIGUOUS DUP_RETURN_DRIFT DUP_SHAPE_AMBIGUOUS EVENT_AFTER_EXIT
    EXECVEAT_AMBIGUOUS EXECVE_SHAPE FCNTL_AMBIGUOUS FD_ANNOTATION_DRIFT FD_CAP
    LEDGER_ADMISSION LEDGER_ARCHIVE LEDGER_CAP LEDGER_LINKER LEDGER_PREFLIGHT
    LEDGER_SCOPE LINKER_EXECVE_NOT_UNIQUE_SUCCESS LINKER_EXIT_NOT_PROVED
    LINKER_EXIT_NOT_ZERO LINKER_PID_NOT_UNIQUE LINKER_PID_REEXEC
    MMAP_SHAPE_AMBIGUOUS OPENAT_DIRFD_AMBIGUOUS OPEN_SHAPE_AMBIGUOUS
    READ_SHAPE_AMBIGUOUS REFUSED_DIRECT_READ_NOT_PROVED REFUSED_MAPPED_ONLY
    RELATIVE_OPEN_AMBIGUOUS RUNTIME_ELF_HASH_DRIFT SPLIT_SYSCALL_AMBIGUOUS
    TRACE_BYTE_CAP TRACE_DIR TRACE_EVENT_CAP TRACE_FILE_CAP
    TRACE_FILE_NOT_REGULAR TRACE_FILE_SCOPE TRACE_PID_DUPLICATE
    TRUNCATED_OR_AMBIGUOUS UNRECOGNIZED_ACTIVE_FD_EVENT
  ]
  if $message in $allowed { $message } else { 'PARSER_OTHER' }
}

def main [ledger: string, trace_dir: string, runtime_elf_sha256: string, receipt: string, private_class: string] {
  try {
    witness $ledger $trace_dir $runtime_elf_sha256 $receipt
  } catch {|err|
    parser_failure_class $err.msg | save --force $private_class
    error make {msg: 'PARSER_REFUSED'}
  }
}
