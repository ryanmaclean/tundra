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
  require ($l.producer_run_id == 42243113451 and $l.producer_artifact_id == 13516402432 and $l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.archive_path == '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a') 'LEDGER_ARCHIVE'
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
    if ($line | str starts-with 'execve(') {
      # Accept only a bounded strace argv list and the pointer-style env form.
      # Truncation of argv is immaterial to executable-path identity.
      let e = (capture $line '^execve\("(?<path>[^"\\]+)", \[(?:(?:"(?:[^"\\]|\\.)*"(?:\.\.\.)?|\.\.\.)(?:, (?:"(?:[^"\\]|\\.)*"(?:\.\.\.)?|\.\.\.))*)?\], (?:0x[0-9a-f]+(?: /\* [0-9]+ vars \*/)?|NULL)\) = (?<result>-?[0-9]+)$')
      require ($e != null) 'EXECVE_SHAPE'
      # Only argv/environment are opaque; the executable path must be complete.
      require (not ($e.path | str contains '...')) 'TRUNCATED_OR_AMBIGUOUS'
      if $e.path == $l.configured_linker_path {
        require (not $exec_ok and not $exited_before_candidate and $e.result == '0') 'LINKER_EXECVE_NOT_UNIQUE_SUCCESS'
        $exec_ok = true
      } else if $exec_ok { error make {msg: 'LINKER_PID_REEXEC'} }
      continue
    }
    require (not ($line | str contains '...') or ($line | str starts-with '+++ exited with ')) 'TRUNCATED_OR_AMBIGUOUS'
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
      # A completed read-only ENOENT allocates no FD and proves no read.
      # Preserve all existing FD and archive witness state.
      let missing = if ($line | str starts-with 'openat(') {
        capture $line '^openat\(AT_FDCWD(?:</[^>\\]*>)?, "(?<path>/[^"\\]*)", O_RDONLY(?:\|O_CLOEXEC)?\) = -1 ENOENT \(No such file or directory\)$'
      } else {
        capture $line '^open\("(?<path>/[^"\\]*)", O_RDONLY(?:\|O_CLOEXEC)?\) = -1 ENOENT \(No such file or directory\)$'
      }
      if $missing != null { continue }
      # A known successful /dev/null open is not an archive witness. Its
      # returned descriptor may replace a previously tracked descriptor.
      let null_open = (capture $line '^open(?:at\(AT_FDCWD(?:</[^>\\]*>)?, |\()"/dev/null", (?<flags>[^)]*)\) = (?<fd>[0-9]{1,9})</dev/null<char 1:3>>$')
      if $null_open != null {
        let flags = ($null_open.flags | split row '|')
        require (($flags | first) == 'O_RDONLY' and (($flags | uniq | length) == ($flags | length)) and ($flags | all {|f| $f in [O_RDONLY O_CLOEXEC O_NOFOLLOW O_NONBLOCK O_LARGEFILE]})) 'OPEN_SHAPE_AMBIGUOUS'
        $active = ($active | where $it != ($null_open.fd | into int))
        continue
      }
      let o = if ($line | str starts-with 'openat(') {
        capture $line '^openat\((?<dirfd>[^,]+), "(?<path>[^"\\]+)", (?<flags>[^)]*)\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$'
      } else {
        capture $line '^open\("(?<path>[^"\\]+)", (?<flags>[^)]*)\) = (?<fd>[0-9]+)<(?<annotation>[^>]+)>$'
      }
      require ($o != null) 'OPEN_SHAPE_AMBIGUOUS'
      if 'dirfd' in $o { require ($o.dirfd =~ '^AT_FDCWD(?:</[^>\\]*>)?$') 'OPENAT_DIRFD_AMBIGUOUS' }
      let fd = ($o.fd | into int)
      $active = ($active | where $it != $fd)
      if not ($o.path | str starts-with '/') {
        # A relative path cannot establish the literal absolute archive open.
        # It may reuse an FD number, so invalidate that number first.
        let flags = ($o.flags | split row '|')
        require (($o.annotation | str starts-with '/') and not ($o.annotation =~ '\\') and ($flags | first) == 'O_RDONLY' and (($flags | uniq | length) == ($flags | length)) and ($flags | all {|f| $f in [O_RDONLY O_CLOEXEC O_NOFOLLOW O_NONBLOCK O_LARGEFILE]})) 'RELATIVE_OPEN_AMBIGUOUS'
        continue
      }
      if $o.path == $l.archive_path {
        # Only known, nonmutating read flags may establish archive custody.
        let flags = ($o.flags | split row '|')
        require ($o.annotation == $l.archive_path and ($flags | first) == 'O_RDONLY' and (($flags | uniq | length) == ($flags | length)) and ($flags | all {|f| $f in [O_RDONLY O_CLOEXEC O_NOFOLLOW O_NONBLOCK O_LARGEFILE]})) 'ARCHIVE_OPEN_NOT_READONLY_EXACT'
        $active = ($active | append $fd)
        $opens += 1
      }
      require (($active | length) <= $l.max_active_fds) 'FD_CAP'
      continue
    }
    if ($line | str starts-with 'close(') {
      # Recognize only completed successful closes; an error may leave the
      # descriptor lifetime ambiguous and must not leave it tracked.
      let c = (capture $line '^close\((?<fd>[0-9]+)(?:<(?<annotation>[^>]+)>)?\)[ \t]{1,64}=[ \t]{1,8}(?<result>0)$')
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
      # An anonymous fd=-1 map does not use an FD and proves no archive read.
      let anon = (capture $line '^mmap\((?<address>NULL|0x[0-9a-f]{1,16}), (?<length>[1-9][0-9]{0,17}), (?<prot>PROT_[A-Z]+(?:\|PROT_[A-Z]+)*), (?<flags>MAP_[A-Z]+(?:\|MAP_[A-Z]+)*), -1, 0\) = (?<result>0x[0-9a-f]{1,16})$')
      if $anon != null {
        let protection = ($anon.prot | split row '|')
        let mapflags = ($anon.flags | split row '|')
        require (($protection | all {|p| $p in [PROT_NONE PROT_READ PROT_WRITE PROT_EXEC]}) and (($protection | uniq | length) == ($protection | length)) and (not ('PROT_NONE' in $protection) or ($protection | length) == 1)) 'MMAP_SHAPE_AMBIGUOUS'
        require (($mapflags | all {|f| $f in [MAP_PRIVATE MAP_SHARED MAP_ANONYMOUS MAP_FIXED]}) and (($mapflags | uniq | length) == ($mapflags | length)) and ('MAP_ANONYMOUS' in $mapflags) and (('MAP_PRIVATE' in $mapflags) != ('MAP_SHARED' in $mapflags)) and (not ('MAP_FIXED' in $mapflags) or ($anon.address != 'NULL' and $anon.address == $anon.result))) 'MMAP_SHAPE_AMBIGUOUS'
        continue
      }
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

export def execve_ellipsis_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let good = 'execve("/emsdk/upstream/bin/wasm-ld", ["arg"...], 0x1) = 0'
  let before = 'execve("/usr/bin/tool", ["literal...name"], 0x1) = 0'
  let open = 'openat(AT_FDCWD, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let finish = '+++ exited with 0 +++'
  let valid = ([$before $good $open $read $finish] | str join "\n")
  let result = (parse_pid $valid 123 $l)
  require ($result.exec_ok and $result.exited and $result.read_positive) 'SELF_TEST_VALID'
  for prefix in [
    'execve("/usr/bin/tool", [], 0x1 /* 12 vars */) = 0'
    'execve("/usr/bin/tool", ["a\"b", "c\\d", ...], 0x1) = 0'
  ] {
    let observed = (parse_pid ([$prefix $good $open $read $finish] | str join "\n") 123 $l)
    require ($observed.exec_ok and $observed.exited and $observed.read_positive) 'SELF_TEST_VALID'
  }
  let cases = [
    {line: 'execve("/emsdk/upstream/bin/wasm-ld...", ["x"], 0x1) = 0', reason: TRUNCATED_OR_AMBIGUOUS}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld\"x", ["x"...], 0x1) = 0', reason: EXECVE_SHAPE}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", ["x"...], 0x1)', reason: EXECVE_SHAPE}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", garbage..., 0x1) = 0', reason: EXECVE_SHAPE}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", ["x"...], 0x1) = 0 extra', reason: EXECVE_SHAPE}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", ["unterminated...], 0x1) = 0', reason: EXECVE_SHAPE}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", ["x"...], 0x1) = -1', reason: LINKER_EXECVE_NOT_UNIQUE_SUCCESS}
    {line: 'execve("/emsdk/upstream/bin/wasm-ld", ["x" <unfinished ...>', reason: SPLIT_SYSCALL_AMBIGUOUS}
    {line: '<... execve resumed>) = 0', reason: SPLIT_SYSCALL_AMBIGUOUS}
    {line: 'open("/tmp/literal...", O_RDONLY) = 3', reason: TRUNCATED_OR_AMBIGUOUS}
    {line: 'notexecve("/x", ["x"...], 0x1) = 0', reason: TRUNCATED_OR_AMBIGUOUS}
    {line: $"($good)\n($good)", reason: LINKER_EXECVE_NOT_UNIQUE_SUCCESS}
    {line: $"($good)\n($before)", reason: LINKER_PID_REEXEC}
  ]
  for case in $cases {
    let actual = (try { parse_pid $case.line 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == $case.reason) 'SELF_TEST_REFUSAL'
  }
  'execve ellipsis parser fixtures passed'
}

export def anonymous_mmap_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let open = 'openat(AT_FDCWD, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let exit = '+++ exited with 0 +++'
  let mapping = 'mmap(NULL, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
  for map in [$mapping 'mmap(NULL, 1, PROT_NONE, MAP_ANONYMOUS|MAP_PRIVATE, -1, 0) = 0x10' 'mmap(NULL, 4096, PROT_READ, MAP_SHARED|MAP_ANONYMOUS, -1, 0) = 0x1000' 'mmap(0x1000, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_FIXED|MAP_ANONYMOUS, -1, 0) = 0x1000' 'mmap(0x2000, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x3000'] {
    let without_read = (parse_pid ([$exec $open $map $exit] | str join "\n") 123 $l)
    require ($without_read.exec_ok and $without_read.exited and not $without_read.read_positive and not $without_read.mapped) 'SELF_TEST_ANON_NO_PROOF'
    let with_read = (parse_pid ([$exec $open $map $read $exit] | str join "\n") 123 $l)
    require ($with_read.exec_ok and $with_read.exited and $with_read.read_positive and not $with_read.mapped) 'SELF_TEST_ANON_WITH_READ'
  }
  let close = 'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = 0'
  let stale = (parse_pid ([$exec $open $mapping $close $read $exit] | str join "\n") 123 $l)
  require (not $stale.read_positive and not $stale.mapped) 'SELF_TEST_ANON_STALE_FD'
  let fixed = 'mmap(0x1000, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_FIXED|MAP_ANONYMOUS, -1, 0) = 0x1000'
  let fixed_prior = (parse_pid ([$exec $open $read $fixed $close $exit] | str join "\n") 123 $l)
  require ($fixed_prior.read_positive and not $fixed_prior.mapped) 'SELF_TEST_ANON_FIXED_PRIOR'
  let fixed_stale = (parse_pid ([$exec $open $fixed $close $read $exit] | str join "\n") 123 $l)
  require (not $fixed_stale.read_positive and not $fixed_stale.mapped) 'SELF_TEST_ANON_FIXED_STALE'
  let reused = 'openat(AT_FDCWD, "/tmp/other", O_RDONLY) = 3</tmp/other>'
  let reused_read = 'read(3</tmp/other>, "") = 1'
  let reuse = (parse_pid ([$exec $open $mapping $close $reused $reused_read $exit] | str join "\n") 123 $l)
  require (not $reuse.read_positive and not $reuse.mapped) 'SELF_TEST_ANON_REUSED_FD'
  let file_map = 'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, 0) = 0x1000'
  let annotated = (parse_pid ([$exec $open $mapping $file_map $exit] | str join "\n") 123 $l)
  require ($annotated.mapped and not $annotated.read_positive) 'SELF_TEST_ANNOTATED_MAP'
  let invalid = [
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0) = 0x1000'
    'mmap(0x1000, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED, -1, 0) = 0x2000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS|MAP_FIXED_NOREPLACE, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_SHARED|MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, 3, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 1) = 0x1000'
    'mmap(NULL, 0, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 9999999999999999999, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x10000000000000000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000 extra'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = -1 ENOMEM (Cannot allocate memory)'
    'mmap(NULL, 4096, PROT_NONE|PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ|PROT_READ, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_OTHER, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
    'mmap(NULL, 4096, PROT_READ, MAP_PRIVATE|MAP_PRIVATE|MAP_ANONYMOUS, -1, 0) = 0x1000'
  ]
  for map in $invalid {
    let actual = (try { parse_pid ([$exec $map] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == 'MMAP_SHAPE_AMBIGUOUS') 'SELF_TEST_ANON_REFUSAL'
  }
  'anonymous mmap parser fixtures passed'
}

export def failed_open_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let opened = 'openat(AT_FDCWD, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let close = 'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = 0'
  let finish = '+++ exited with 0 +++'
  let archive_missing = 'openat(AT_FDCWD, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = -1 ENOENT (No such file or directory)'
  let missing = 'openat(AT_FDCWD, "/tmp/fixture-missing", O_RDONLY|O_CLOEXEC) = -1 ENOENT (No such file or directory)'
  let annotated_missing = 'openat(AT_FDCWD</tmp>, "/tmp/fixture-missing", O_RDONLY|O_CLOEXEC) = -1 ENOENT (No such file or directory)'
  let other_missing = 'open("/tmp/fixture-missing", O_RDONLY) = -1 ENOENT (No such file or directory)'
  for absent in [$archive_missing $missing $annotated_missing $other_missing] {
    let no_read = (parse_pid ([$exec $absent $finish] | str join "\n") 123 $l)
    require (not $no_read.read_positive and $no_read.direct_archive_opens == 0) 'SELF_TEST_FAILED_OPEN_NO_PROOF'
    let before_read = (parse_pid ([$exec $opened $absent $read $finish] | str join "\n") 123 $l)
    require ($before_read.read_positive and $before_read.direct_archive_opens == 1) 'SELF_TEST_FAILED_OPEN_ACTIVE'
    let stale = (parse_pid ([$exec $opened $close $absent $read $finish] | str join "\n") 123 $l)
    require (not $stale.read_positive) 'SELF_TEST_FAILED_OPEN_STALE'
  }
  let reused = 'openat(AT_FDCWD, "/tmp/other", O_RDONLY) = 3</tmp/other>'
  let reused_read = 'read(3</tmp/other>, "") = 1'
  let reuse = (parse_pid ([$exec $opened $close $missing $reused $reused_read $finish] | str join "\n") 123 $l)
  require (not $reuse.read_positive) 'SELF_TEST_FAILED_OPEN_REUSED'
  let invalid = [
    'openat(AT_FDCWD, "relative", O_RDONLY) = -1 ENOENT (No such file or directory)'
    'openat(3</tmp>, "/tmp/x", O_RDONLY) = -1 ENOENT (No such file or directory)'
    'openat(AT_FDCWD, "/tmp/x\"y", O_RDONLY) = -1 ENOENT (No such file or directory)'
    'openat(AT_FDCWD, "/tmp/x", O_WRONLY) = -1 ENOENT (No such file or directory)'
    'openat(AT_FDCWD, "/tmp/x", O_RDONLY|O_CREAT) = -1 ENOENT (No such file or directory)'
    'openat(AT_FDCWD, "/tmp/x", O_RDONLY) = -1 EACCES (Permission denied)'
    'openat(AT_FDCWD, "/tmp/x", O_RDONLY) = -1 EINTR (Interrupted system call)'
    'openat(AT_FDCWD, "/tmp/x", O_RDONLY) = ? ERESTARTSYS (To be restarted if SA_RESTART is set)'
    'openat(AT_FDCWD, "/tmp/x", O_RDONLY) = -1 ENOENT (No such file or directory) extra'
  ]
  for line in $invalid {
    let actual = (try { parse_pid ([$exec $line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == 'OPEN_SHAPE_AMBIGUOUS') 'SELF_TEST_FAILED_OPEN_REFUSAL'
  }
  let dotted = 'openat(AT_FDCWD, "/tmp/absent...", O_RDONLY) = -1 ENOENT (No such file or directory)'
  let split = 'openat(AT_FDCWD, "/tmp/absent", O_RDONLY <unfinished ...>'
  for case in [{line: $dotted, reason: TRUNCATED_OR_AMBIGUOUS} {line: $split, reason: SPLIT_SYSCALL_AMBIGUOUS}] {
    let actual = (try { parse_pid ([$exec $case.line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == $case.reason) 'SELF_TEST_FAILED_OPEN_PRECHECK'
  }
  'failed open parser fixtures passed'
}

export def openat_cwd_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let opened = 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY|O_CLOEXEC) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let legacy = 'openat(AT_FDCWD, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let close = 'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = 0'
  let finish = '+++ exited with 0 +++'
  for open in [$opened $legacy] {
    let no_read = (parse_pid ([$exec $open $finish] | str join "\n") 123 $l)
    require ($no_read.direct_archive_opens == 1 and not $no_read.read_positive) 'SELF_TEST_CWD_NO_READ'
    let positive = (parse_pid ([$exec $open $read $finish] | str join "\n") 123 $l)
    require ($positive.direct_archive_opens == 1 and $positive.read_positive) 'SELF_TEST_CWD_READ'
    let stale = (parse_pid ([$exec $open $close $read $finish] | str join "\n") 123 $l)
    require (not $stale.read_positive) 'SELF_TEST_CWD_STALE'
    let other = 'openat(AT_FDCWD</tmp>, "/tmp/other", O_RDONLY) = 3</tmp/other>'
    let other_read = 'read(3</tmp/other>, "") = 1'
    let reused = (parse_pid ([$exec $open $close $other $other_read $finish] | str join "\n") 123 $l)
    require (not $reused.read_positive) 'SELF_TEST_CWD_REUSED'
  }
  let invalid = [
    {line: 'openat(3</tmp>, "/tmp/a", O_RDONLY) = 4</tmp/a>', reason: OPENAT_DIRFD_AMBIGUOUS}
    {line: 'openat(AT_FDCWD<relative>, "/tmp/a", O_RDONLY) = 4</tmp/a>', reason: OPENAT_DIRFD_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp\x>, "/tmp/a", O_RDONLY) = 4</tmp/a>', reason: OPENAT_DIRFD_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY|O_TRUNC) = 4</tmp/a>', reason: RELATIVE_OPEN_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "/tmp/a\"b", O_RDONLY) = 4</tmp/a>', reason: OPEN_SHAPE_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</tmp/other>', reason: ARCHIVE_OPEN_NOT_READONLY_EXACT}
    {line: 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_WRONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>', reason: ARCHIVE_OPEN_NOT_READONLY_EXACT}
    {line: 'openat(AT_FDCWD</tmp>, "/tmp/a", O_RDONLY) = -1 EACCES (Permission denied)', reason: OPEN_SHAPE_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "/tmp/a", O_RDONLY)', reason: OPEN_SHAPE_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "/tmp/absent...", O_RDONLY) = 4</tmp/a>', reason: TRUNCATED_OR_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "/tmp/a", O_RDONLY <unfinished ...>', reason: SPLIT_SYSCALL_AMBIGUOUS}
  ]
  for case in $invalid {
    let actual = (try { parse_pid ([$exec $case.line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == $case.reason) 'SELF_TEST_CWD_REFUSAL'
  }
  'openat cwd annotation fixtures passed'
}

export def archive_flags_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let path = '/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let finish = '+++ exited with 0 +++'
  for flags in ['O_RDONLY' 'O_RDONLY|O_CLOEXEC' 'O_RDONLY|O_NOFOLLOW' 'O_RDONLY|O_NONBLOCK' 'O_RDONLY|O_LARGEFILE' 'O_RDONLY|O_NOFOLLOW|O_NONBLOCK|O_LARGEFILE'] {
    let open = (['openat(AT_FDCWD</tmp>, "' $path '", ' $flags ') = 3<' $path '>'] | str join)
    let result = (parse_pid ([$exec $open $read $finish] | str join "\n") 123 $l)
    require ($result.direct_archive_opens == 1 and $result.read_positive) 'SELF_TEST_FLAGS_READ'
  }
  for flags in ['O_RDONLY|O_TRUNC' 'O_RDONLY|O_CREAT' 'O_RDONLY|O_TMPFILE' 'O_RDONLY|O_PATH' 'O_RDONLY|O_RDWR' 'O_RDONLY|O_WRONLY' 'O_RDONLY|O_UNKNOWN' 'O_RDONLY|O_CLOEXEC|O_CLOEXEC' 'O_RDONLY|0x1' 'O_RDONLY|' 'O_RDONLY| O_CLOEXEC' 'O_CLOEXEC|O_RDONLY' 'O_CLOEXEC' 'O_RDWR'] {
    let open = (['openat(AT_FDCWD</tmp>, "' $path '", ' $flags ') = 3<' $path '>'] | str join)
    let actual = (try { parse_pid ([$exec $open] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == 'ARCHIVE_OPEN_NOT_READONLY_EXACT') 'SELF_TEST_FLAGS_REFUSAL'
  }
  'archive open flag fixtures passed'
}

export def close_alignment_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let opened = 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let other = 'openat(AT_FDCWD</tmp>, "/tmp/other", O_RDONLY) = 3</tmp/other>'
  let other_read = 'read(3</tmp/other>, "") = 1'
  let finish = '+++ exited with 0 +++'
  let close = 'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>)'
  let aligned = ([$close (' ' | fill -w 30) '= 0'] | str join)
  let tabs = ([$close "\t=\t0"] | str join)
  for line in [$aligned $tabs] {
    let stale = (parse_pid ([$exec $opened $line $read $finish] | str join "\n") 123 $l)
    require (not $stale.read_positive) 'SELF_TEST_CLOSE_STALE'
    let earlier = (parse_pid ([$exec $opened $read $line $finish] | str join "\n") 123 $l)
    require $earlier.read_positive 'SELF_TEST_CLOSE_PRIOR_READ'
    let reused = (parse_pid ([$exec $opened $line $other $other_read $finish] | str join "\n") 123 $l)
    require (not $reused.read_positive) 'SELF_TEST_CLOSE_REUSED'
    let reopened = (parse_pid ([$exec $opened $line $opened $read $finish] | str join "\n") 123 $l)
    require $reopened.read_positive 'SELF_TEST_CLOSE_REOPENED'
  }
  let invalid = [
    'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = -1'
    'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = 1'
    'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = -1 EINTR (Interrupted system call)'
    'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>) = -1 EBADF (Bad file descriptor)'
    'close(3</bad) = 0'
    'close(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>)=0'
    ([$close (' ' | fill -w 65) '= 0'] | str join)
    ([$close '= ' (' ' | fill -w 9) '0'] | str join)
  ]
  for line in $invalid {
    let actual = (try { parse_pid ([$exec $opened $line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == 'CLOSE_SHAPE_AMBIGUOUS') 'SELF_TEST_CLOSE_REFUSAL'
  }
  let drift = 'close(3</tmp/other>) = 0'
  let drift_result = (try { parse_pid ([$exec $opened $drift] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
  require ($drift_result == 'FD_ANNOTATION_DRIFT') 'SELF_TEST_CLOSE_ANNOTATION'
  'close alignment parser fixtures passed'
}

export def relative_open_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let archive = 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let alias = 'openat(AT_FDCWD</tmp>, "relative/libnoexit.a", O_RDONLY|O_CLOEXEC) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let alias_read = 'read(3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>, "") = 1'
  let plain_read = 'read(3, "") = 1'
  let finish = '+++ exited with 0 +++'
  let only_alias = (parse_pid ([$exec $alias $alias_read $finish] | str join "\n") 123 $l)
  require ($only_alias.direct_archive_opens == 0 and not $only_alias.read_positive) 'SELF_TEST_RELATIVE_ALIAS'
  let reused = (parse_pid ([$exec $archive $alias $plain_read $finish] | str join "\n") 123 $l)
  require (not $reused.read_positive) 'SELF_TEST_RELATIVE_REUSED'
  let other = 'openat(AT_FDCWD</tmp>, "relative/other", O_RDONLY) = 4</tmp/other>'
  let independent = (parse_pid ([$exec $archive $other $alias_read $finish] | str join "\n") 123 $l)
  require $independent.read_positive 'SELF_TEST_RELATIVE_OTHER_FD'
  let after = (parse_pid ([$exec $alias $archive $alias_read $finish] | str join "\n") 123 $l)
  require $after.read_positive 'SELF_TEST_RELATIVE_SEPARATE_ARCHIVE'
  let invalid = [
    {line: 'openat(3</tmp>, "relative", O_RDONLY) = 4</tmp/other>', reason: OPENAT_DIRFD_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative\\path", O_RDONLY) = 4</tmp/other>', reason: OPEN_SHAPE_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY|O_TRUNC) = 4</tmp/other>', reason: RELATIVE_OPEN_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY|O_CREAT) = 4</tmp/other>', reason: RELATIVE_OPEN_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY) = 4<relative>', reason: RELATIVE_OPEN_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY) = 4</tmp\other>', reason: RELATIVE_OPEN_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative...", O_RDONLY) = 4</tmp/other>', reason: TRUNCATED_OR_AMBIGUOUS}
    {line: 'openat(AT_FDCWD</tmp>, "relative", O_RDONLY <unfinished ...>', reason: SPLIT_SYSCALL_AMBIGUOUS}
  ]
  for case in $invalid {
    let actual = (try { parse_pid ([$exec $case.line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == $case.reason) 'SELF_TEST_RELATIVE_REFUSAL'
  }
  'relative non-witness open fixtures passed'
}

export def nested_device_self_test [ledger_path: string] {
  let l = (checked_ledger $ledger_path)
  let exec = 'execve("/emsdk/upstream/bin/wasm-ld", [], 0x1) = 0'
  let archive = 'openat(AT_FDCWD</tmp>, "/work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a", O_RDONLY) = 3</work/image/cache/sysroot/lib/wasm32-emscripten/libnoexit.a>'
  let device = 'openat(AT_FDCWD</tmp>, "/dev/null", O_RDONLY|O_CLOEXEC) = 3</dev/null<char 1:3>>'
  let read = 'read(3, "") = 1'
  let end = '+++ exited with 0 +++'
  let alone = (parse_pid ([$exec $device $read $end] | str join "\n") 123 $l)
  require ($alone.direct_archive_opens == 0 and not $alone.read_positive) 'SELF_TEST_DEVICE_NO_WITNESS'
  let reused = (parse_pid ([$exec $archive $device $read $end] | str join "\n") 123 $l)
  require (not $reused.read_positive) 'SELF_TEST_DEVICE_REUSE'
  let other = 'open("/dev/null", O_RDONLY) = 4</dev/null<char 1:3>>'
  let independent = (parse_pid ([$exec $archive $other $read $end] | str join "\n") 123 $l)
  require $independent.read_positive 'SELF_TEST_DEVICE_OTHER_FD'
  for line in [
    'openat(3</tmp>, "/dev/null", O_RDONLY) = 3</dev/null<char 1:3>>'
    'openat(AT_FDCWD, "/dev/zero", O_RDONLY) = 3</dev/zero<char 1:5>>'
    'openat(AT_FDCWD, "/dev/null", O_RDONLY) = 3</dev/null<char 1:5>>'
    'openat(AT_FDCWD, "/dev/null", O_RDONLY|O_TRUNC) = 3</dev/null<char 1:3>>'
    'openat(AT_FDCWD, "/dev/null", O_RDONLY|O_RDONLY) = 3</dev/null<char 1:3>>'
    'openat(AT_FDCWD, "/dev/null", O_RDONLY) = -1 EBADF (Bad file descriptor)'
    'openat(AT_FDCWD, "/dev/null", O_RDONLY) = 3</dev/null<char 1:3>> trailing'
  ] {
    let actual = (try { parse_pid ([$exec $line] | str join "\n") 123 $l; 'ACCEPTED' } catch {|err| $err.msg })
    require ($actual == 'OPEN_SHAPE_AMBIGUOUS') 'SELF_TEST_DEVICE_REFUSAL'
  }
  'nested device non-witness fixtures passed'
}

def main [ledger: string, trace_dir: string, runtime_elf_sha256: string, receipt: string, private_class: string] {
  try {
    witness $ledger $trace_dir $runtime_elf_sha256 $receipt
  } catch {|err|
    parser_failure_class $err.msg | save --force $private_class
    error make {msg: 'PARSER_REFUSED'}
  }
}
