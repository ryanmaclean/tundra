#!/usr/bin/env nu

# Structural and custody preflight only. Never installs or executes a .deb.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def sha [path: string] {
  let result = (^sha256sum -- $path | complete)
  require ($result.exit_code == 0) 'HASH_FAILED'
  ($result.stdout | split row ' ' | first)
}
def regular_size [path: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') 'INPUT_NOT_REGULAR'
  ($rows | first).size | into int
}
def checked_ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-sdk-libnoexit-os-install-preflight/v1' and $l.scope == 'FOUR_SAME_RUN_ARTIFACT_AND_DEB_STRUCTURE_ONLY') 'LEDGER_SCOPE'
  require ($l.repository == 'ryanmaclean/tundra' and $l.repository_id == 1164936674 and $l.source_head == '2a50c1fedb04c1a62acc034dde956403550566f2' and $l.run_id == 40548891504 and $l.run_attempt == 1) 'LEDGER_RUN'
  require ($l.producer_run == 40536996581 and $l.producer_artifact_id == 12748008032 and $l.producer_zip_sha256 == '0f051bf7adec09d86d786e5642c4ef89018da6958ab04094264c52ae8bd7ab57') 'LEDGER_PRODUCER'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'LEDGER_RESOURCE'
  require ($l.ledger_member_sha256 == 'ca8cbbd0f65be4e9645ff2d308c24ba0cbe21172bfe20bf005d4a761c1f6e2eb' and $l.preflight_member_sha256 == 'd34398146682469b1f5d33d4a904ebfbedb8669ec562afc5d8a1f02f3237f34e') 'LEDGER_MEMBER'
  require ($l.max_artifact_zip_bytes == 20971520 and $l.max_deb_bytes == 201326592 and $l.max_data_members == 4096 and $l.max_data_expanded_bytes == 100663296 and $l.max_control_expanded_bytes == 131072) 'LEDGER_CAP'
  require ($l.data_path_manifest_sha256 == '22036562c0db17f76a7fce50fd30021ee3eb782518e6bb59a8dbd52bdfb213f2') 'LEDGER_PATH_MANIFEST'
  require ($l.expected_package.name == 'auto-tundra' and $l.expected_package.version == '0.1.0' and $l.expected_package.architecture == 'amd64' and $l.expected_package.depends == 'libwebkit2gtk-4.1-0, libgtk-3-0') 'LEDGER_PACKAGE'
  require ($l.expected_zip_members == ['ledger.json' 'package.deb' 'preflight.json' 'witness/cell-witness.json' 'witness/installed-archive' 'witness/installed-cell.json' 'witness/installed-notice']) 'LEDGER_ZIP_MEMBERS'
  require (($l.cells | length) == 4 and ($l.cells | each {|c| $"($c.condition)/($c.profile)"}) == ['image/default' 'image/sounds' 'clean/default' 'clean/sounds']) 'LEDGER_CELLS'
  require ($l.package_install_verified == 'NO' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_ADMISSION'
  $l
}

def deb_structure [deb: string, l: record, c: record] {
  # Python's stdlib parses ar and tar in memory; no package member is extracted or run.
  let py = '
import hashlib, io, json, re, struct, sys, tarfile

def fail(code):
    print(code, file=sys.stderr)
    raise SystemExit(1)
def sha(b): return hashlib.sha256(b).hexdigest()
try:
    p, condition, profile, cell_sha, notice_sha, archive_sha, elf_sha, path_manifest_sha = sys.argv[1:9]
    b = open(p, "rb").read()
    if not b.startswith(b"!<arch>\n"): fail("DEB_AR_MAGIC")
    pos = 8; parts = {}
    while pos < len(b):
        if pos + 60 > len(b): fail("DEB_AR_TRUNCATED")
        h = b[pos:pos+60]; pos += 60
        if h[58:60] != b"`\n": fail("DEB_AR_HEADER")
        name = h[:16].decode("ascii").strip().rstrip("/")
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", name) or name in parts: fail("DEB_AR_MEMBER")
        size = int(h[48:58].decode("ascii").strip())
        if size < 0 or pos + size > len(b): fail("DEB_AR_SIZE")
        parts[name] = b[pos:pos+size]; pos += size
        if size & 1:
            if pos >= len(b) or b[pos:pos+1] != b"\n": fail("DEB_AR_PADDING")
            pos += 1
    if pos != len(b) or set(parts) != {"debian-binary", "control.tar.gz", "data.tar.gz"} or parts["debian-binary"] != b"2.0\n": fail("DEB_AR_SCOPE")
    ctl=parts["control.tar.gz"]
    if len(ctl)>131072: fail("CONTROL_CAP")
    with tarfile.open(fileobj=io.BytesIO(ctl),mode="r:gz") as t:
        ms=t.getmembers(); names=[m.name.removeprefix("./") for m in ms]
        if set(names)!={"control","md5sums"} or len(names)!=2 or sum(m.size for m in ms)>131072 or any(not m.isfile() or m.pax_headers or m.uid!=0 or m.gid!=0 for m in ms): fail("MAINTAINER_SCRIPT_OR_CONTROL_DRIFT")
        control=t.extractfile(next(m for m in ms if m.name.removeprefix("./")=="control")).read().decode("utf-8")
        fields={}; previous=None
        for line in control.splitlines():
            if not line: fail("CONTROL_SHAPE")
            if line.startswith(" "):
                if previous is None: fail("CONTROL_FOLDING")
                fields[previous] += "\n" + line[1:]
                continue
            if ": " not in line: fail("CONTROL_SHAPE")
            k,v=line.split(": ",1)
            if not re.fullmatch(r"[A-Za-z0-9-]+",k): fail("CONTROL_SHAPE")
            k=k.lower()
            if k in fields: fail("CONTROL_DUPLICATE")
            fields[k]=v; previous=k
        if set(fields)!={"package","version","architecture","installed-size","maintainer","priority","depends","description"}: fail("CONTROL_FIELDS")
        if any(fields.get(k)!=v for k,v in {"package":"auto-tundra","version":"0.1.0","architecture":"amd64","maintainer":"autotundra","priority":"optional","depends":"libwebkit2gtk-4.1-0, libgtk-3-0","description":"(none)\n(none)"}.items()): fail("CONTROL_IDENTITY")
        if not re.fullmatch(r"[0-9]{1,8}",fields["installed-size"]): fail("CONTROL_INSTALLED_SIZE")
    with tarfile.open(fileobj=io.BytesIO(parts["data.tar.gz"]),mode="r:gz") as t:
        seen=set(); total=0; count=0; path_rows=[]; wanted={
          "usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime/native-emscripten-LICENSE":notice_sha,
          "usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime/native-emscripten-libnoexit.a":archive_sha,
          "usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime/native-emscripten-libnoexit-cell.json":cell_sha}
        found={}; elf=[]
        for m in t:
            count += 1
            if count>4096: fail("DATA_MEMBER_CAP")
            name=m.name.removeprefix("./")
            if not name or name.startswith("/") or "\\" in name or any(x in ("", ".", "..") for x in name.split("/")) or len(name)>256 or not re.fullmatch(r"[A-Za-z0-9_./+~:-]+", name): fail("DATA_PATH")
            if name in seen or m.pax_headers or m.uid!=0 or m.gid!=0 or m.mode & 0o6000: fail("DATA_METADATA")
            seen.add(name)
            if not (m.isfile() or m.isdir()) or not (name=="usr" or name.startswith("usr/")): fail("DATA_TYPE_SCOPE")
            path_rows.append((name,"f" if m.isfile() else "d"))
            total += m.size
            if total>100663296: fail("DATA_EXPANDED_CAP")
            if name in wanted or name=="usr/bin/auto-tundra":
                if not m.isfile(): fail("DATA_SELECTED_NOT_REGULAR")
                data=t.extractfile(m).read()
                if name in wanted:
                    if sha(data)!=wanted[name]: fail("DATA_SELECTED_BYTES")
                    found[name]=sha(data)
                    if name.endswith("cell.json"):
                        cell=json.loads(data)
                        if cell.get("condition")!=condition or cell.get("profile")!=profile or cell.get("package_install_verified") not in (None,"NO") or cell.get("archive_linker_input")!="UNPROVEN" or any(cell.get(k)!="NO" for k in ("legal_admission","product_admission","fleet_admission")): fail("CELL_SCOPE")
                else:
                    if m.mode != 0o755 or len(data)<64 or data[:7] != b"\x7fELF\x02\x01\x01" or struct.unpack_from("<HHI",data,16)!=(3,62,1): fail("ELF_FORMAT_MACHINE")
                    if sha(data)!=elf_sha: fail("ELF_BYTES_DRIFT")
                    elf.append(sha(data))
        manifest="".join(f"{name}\t{kind}\n" for name,kind in sorted(path_rows)).encode()
        if sha(manifest)!=path_manifest_sha: fail("DATA_PATH_MANIFEST")
        if set(found)!=set(wanted) or len(elf)!=1: fail("DATA_SELECTED_MISSING")
        print(json.dumps({"status":"DEB_STRUCTURE_ONLY","package_sha256":sha(b),"control_scripts":"NONE","data_members":count,"data_expanded_bytes":total,"installed_elf_sha256":elf[0],"selected_resource_sha256":found,"package_install_verified":"NO","linker_input":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
except (ValueError, UnicodeError, OSError, tarfile.TarError, KeyError, json.JSONDecodeError): fail("DEB_PARSE_REFUSED")
'
  let result = (^python3 -c $py $deb $c.condition $c.profile $c.cell_resource_sha256 $l.notice_sha256 $l.archive_sha256 $c.elf_sha256 $l.data_path_manifest_sha256 | complete)
  require ($result.exit_code == 0) 'DEB_STRUCTURE_REFUSED'
  $result.stdout | from json
}

export def inspect_cell [ledger_path: string, condition: string, profile: string, run_path: string, artifact_path: string, zip_path: string, receipt_path: string] {
  let l = (checked_ledger $ledger_path)
  let rows = ($l.cells | where condition == $condition and profile == $profile)
  require (($rows | length) == 1) 'CELL_NOT_UNIQUE'
  let c = ($rows | first)
  require (($c.artifact_id | describe) == 'int' and ($c.artifact_zip_bytes | describe) == 'int' and $c.artifact_zip_sha256 =~ '^[0-9a-f]{64}$' and $c.deb_sha256 =~ '^[0-9a-f]{64}$' and $c.cell_resource_sha256 =~ '^[0-9a-f]{64}$' and $c.elf_sha256 =~ '^[0-9a-f]{64}$') 'CELL_TUPLE_PENDING'
  require ($c.artifact_name == $"duckdb-libnoexit-installed-($l.run_id)-($l.run_attempt)-($condition)-($profile)" and $c.cell_key == $"($condition)/sdk/libnoexit.a/__archive__/1") 'CELL_IDENTITY'
  let run = (open $run_path)
  let artifact = (open $artifact_path)
  require ($run.id == $l.run_id and $run.run_attempt == $l.run_attempt and $run.head_sha == $l.source_head and $run.event == 'workflow_dispatch' and $run.repository.id == $l.repository_id and $run.head_repository.id == $l.repository_id) 'RUN_API_DRIFT'
  require ($artifact.id == $c.artifact_id and $artifact.name == $c.artifact_name and $artifact.size_in_bytes == $c.artifact_zip_bytes and $artifact.digest == $"sha256:($c.artifact_zip_sha256)" and $artifact.expired == false) 'ARTIFACT_API_DRIFT'
  require ($artifact.workflow_run.id == $l.run_id and $artifact.workflow_run.head_sha == $l.source_head and $artifact.workflow_run.repository_id == $l.repository_id and $artifact.workflow_run.head_repository_id == $l.repository_id) 'ARTIFACT_RUN_JOIN'
  require ((regular_size $zip_path) == $c.artifact_zip_bytes and $c.artifact_zip_bytes <= $l.max_artifact_zip_bytes and (sha $zip_path) == $c.artifact_zip_sha256) 'ZIP_BYTES_DRIFT'
  let crc = (^unzip -tqq $zip_path | complete)
  require ($crc.exit_code == 0) 'ZIP_CRC'
  let zip_shape = (^python3 -c 'import sys,zipfile,stat
z=zipfile.ZipFile(sys.argv[1]); i=z.infolist()
names=[x.filename for x in i]
expected=["ledger.json","package.deb","preflight.json","witness/cell-witness.json","witness/installed-archive","witness/installed-cell.json","witness/installed-notice"]
assert names==expected and len(names)==len(set(names))
assert all(stat.S_ISREG(x.external_attr>>16) and not x.flag_bits&1 and x.file_size<=201326592 for x in i)
assert sum(x.file_size for x in i)<=202375168
assert z.testzip() is None' $zip_path | complete)
  require ($zip_shape.exit_code == 0) 'ZIP_SHAPE'
  let listing = (^unzip -Z -1 $zip_path | complete)
  require ($listing.exit_code == 0 and ($listing.stdout | lines) == $l.expected_zip_members) 'ZIP_MEMBERS'
  let created = (^mktemp -d | complete)
  require ($created.exit_code == 0) 'SCRATCH_CREATE'
  let scratch = ($created.stdout | str trim)
  require (($scratch | path basename) =~ '^tmp\.[A-Za-z0-9]{6,}$' and ((try { ls -lD $scratch | first | get type } catch { '' }) == 'dir')) 'SCRATCH_SCOPE'
  try {
    for member in ['ledger.json' 'preflight.json' 'witness/cell-witness.json' 'witness/installed-cell.json' 'witness/installed-archive' 'witness/installed-notice' 'package.deb'] {
      let path = ($scratch | path join ($member | str replace '/' '_'))
      ^unzip -p $zip_path $member o> $path
      require ($env.LAST_EXIT_CODE == 0) 'ZIP_MEMBER_EXTRACT'
    }
    require ((sha ($scratch | path join 'ledger.json')) == $l.ledger_member_sha256 and (sha ($scratch | path join 'preflight.json')) == $l.preflight_member_sha256) 'FROZEN_MEMBER_DRIFT'
    require ((sha ($scratch | path join 'witness_installed-archive')) == $l.archive_sha256 and (sha ($scratch | path join 'witness_installed-notice')) == $l.notice_sha256 and (sha ($scratch | path join 'witness_installed-cell.json')) == $c.cell_resource_sha256) 'WITNESS_MEMBER_DRIFT'
    let witness = (open ($scratch | path join 'witness_cell-witness.json'))
    require ($witness.condition == $condition and $witness.profile == $profile and $witness.key == $c.cell_key and $witness.package_sha256 == $c.deb_sha256 and $witness.archive_sha256 == $l.archive_sha256 and $witness.notice_sha256 == $l.notice_sha256 and $witness.producer_run == $l.producer_run and $witness.producer_artifact_id == $l.producer_artifact_id and $witness.producer_zip_sha256 == $l.producer_zip_sha256 and $witness.package_extraction_verified == 'YES' and $witness.package_install_verified == 'NO' and $witness.linker_input == 'UNPROVEN' and $witness.legal_admission == 'NO' and $witness.product_admission == 'NO' and $witness.fleet_admission == 'NO') 'WITNESS_SCOPE'
    let deb = ($scratch | path join 'package.deb')
    require ((regular_size $deb) == $witness.package_bytes and (regular_size $deb) <= $l.max_deb_bytes and (sha $deb) == $c.deb_sha256) 'DEB_BYTES_DRIFT'
    let structure = (deb_structure $deb $l $c)
    {schema: $l.schema, status: 'CELL_ARTIFACT_AND_DEB_STRUCTURE_ONLY', condition: $condition, profile: $profile, artifact_id: $c.artifact_id, artifact_zip_sha256: $c.artifact_zip_sha256, package_sha256: $c.deb_sha256, structure: $structure, package_install_verified: 'NO', linker_input: 'UNPROVEN', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt_path
  } finally {
    let cleanup = (^rm -rf -- $scratch | complete)
    require ($cleanup.exit_code == 0 and (try { ls -lD $scratch | is-empty } catch { true })) 'SCRATCH_CLEANUP'
  }
}

export def inspect_deb [ledger_path: string, condition: string, profile: string, deb_path: string, receipt_path: string] {
  let l = (checked_ledger $ledger_path)
  let rows = ($l.cells | where condition == $condition and profile == $profile)
  require (($rows | length) == 1) 'CELL_NOT_UNIQUE'
  let c = ($rows | first)
  require (($c.deb_sha256 | describe) == 'string' and $c.deb_sha256 =~ '^[0-9a-f]{64}$' and $c.elf_sha256 =~ '^[0-9a-f]{64}$') 'CELL_TUPLE_PENDING'
  require ((regular_size $deb_path) <= $l.max_deb_bytes and (sha $deb_path) == $c.deb_sha256) 'DEB_BYTES_DRIFT'
  let structure = (deb_structure $deb_path $l $c)
  {schema: $l.schema, status: 'STANDALONE_DEB_STRUCTURE_ONLY', condition: $condition, profile: $profile, structure: $structure, package_install_verified: 'NO', linker_input: 'UNPROVEN', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt_path
}

export def all_cells [ledger_path: string, run_path: string, artifacts_root: string, receipt_path: string] {
  let l = (checked_ledger $ledger_path)
  require ($l.runnable == 'YES') 'FOURTH_CELL_PENDING_RUNNABLE_NO'
  let run = (open $run_path)
  require ($run.status == 'completed' and $run.conclusion == 'success') 'RUN_NOT_TERMINAL_SUCCESS'
  mut receipts = []
  for c in $l.cells {
    let dir = ($artifacts_root | path join $"($c.condition)-($c.profile)")
    let out = ($dir | path join 'cell-preflight.json')
    inspect_cell $ledger_path $c.condition $c.profile $run_path ($dir | path join 'artifact.json') ($dir | path join 'artifact.zip') $out
    $receipts = ($receipts | append (open $out))
  }
  require (($receipts | length) == 4 and ($receipts | get status | uniq) == ['CELL_ARTIFACT_AND_DEB_STRUCTURE_ONLY']) 'FOUR_CELL_RESULT'
  {schema: $l.schema, status: 'FOUR_ARTIFACTS_AND_DEB_STRUCTURES_ONLY', run_id: $l.run_id, cells: ($receipts | select condition profile artifact_id artifact_zip_sha256 package_sha256), package_install_verified: 'NO', linker_input: 'UNPROVEN', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'} | to json --raw | save $receipt_path
}

def main [mode: string, ledger: string, ...args: string] {
  if $mode == 'inspect-cell' {
    require (($args | length) == 6) 'ARGUMENTS'
    inspect_cell $ledger $args.0 $args.1 $args.2 $args.3 $args.4 $args.5
  } else if $mode == 'all' {
    require (($args | length) == 3) 'ARGUMENTS'
    all_cells $ledger $args.0 $args.1 $args.2
  } else if $mode == 'inspect-deb' {
    require (($args | length) == 4) 'ARGUMENTS'
    inspect_deb $ledger $args.0 $args.1 $args.2 $args.3
  } else { error make {msg: 'MODE'} }
}
