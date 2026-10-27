#!/usr/bin/env nu

# Only a disposable-runner dpkg ownership/byte witness. No app execution or smoke.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def sha [path: string] {
  let r = (^sha256sum -- $path | complete)
  require ($r.exit_code == 0) 'HASH_FAILED'
  $r.stdout | split row ' ' | first
}
def regular [path: string] {
  let r = (try { ls -lD $path } catch { [] })
  require (($r | length) == 1 and ($r | first).type == 'file') 'INPUT_NOT_REGULAR'
  ($r | first).size | into int
}
def main [ledger_path: string, condition: string, profile: string, deb: string, preflight_path: string, receipt_path: string, refusal_path: string] {
  let l = (open $ledger_path)
  require ($l.schema == 'tundra-sdk-libnoexit-os-install-witness/v1' and $l.scope == 'FOUR_AUTHENTICATED_PR148_PACKAGES_AND_DISPOSABLE_OS_INSTALL_ONLY') 'LEDGER_SCOPE'
  require ($l.run_id == 40548891504 and $l.run_attempt == 1 and $l.source_head == '2a50c1fedb04c1a62acc034dde956403550566f2' and $l.repository_id == 1164936674) 'LEDGER_RUN'
  require ($l.source_preflight_helper_sha256 == 'f070573bd130334ad2ccadb3661795bb6276a8547691616f00432ffc8cd79247' and $l.source_preflight_ledger_sha256 == 'bed8ac3ef50793c49990db967d8bcedeb87d6a50be905f08c90c35148c290b58') 'LEDGER_PREFLIGHT_PIN'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'LEDGER_RESOURCE'
  require ($l.max_artifact_zip_bytes == 20971520 and $l.max_deb_bytes == 201326592 and $l.max_receipt_bytes == 8192) 'LEDGER_CAP'
  require ($l.behavior_cases == 'NOT_RUN' and $l.package_install_verified == 'PENDING_HOSTED_WITNESS' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_ADMISSION'
  require (($l.cells | length) == 4 and ($l.cells | each {|c| $"($c.condition)/($c.profile)"}) == ['image/default' 'image/sounds' 'clean/default' 'clean/sounds']) 'LEDGER_CELLS'
  let rows = ($l.cells | where condition == $condition and profile == $profile)
  require (($rows | length) == 1) 'CELL_UNIQUE'
  let c = ($rows | first)
  require (($c.artifact_id | describe) == 'int' and $c.artifact_zip_sha256 =~ '^[0-9a-f]{64}$' and $c.deb_sha256 =~ '^[0-9a-f]{64}$' and $c.cell_resource_sha256 =~ '^[0-9a-f]{64}$' and $c.elf_sha256 =~ '^[0-9a-f]{64}$') 'CELL_TUPLE'
  let p = (open $preflight_path)
  require ($p.status == 'FOUR_ARTIFACTS_AND_DEB_STRUCTURES_ONLY' and $p.run_id == $l.run_id and ($p.cells | length) == 4 and $p.package_install_verified == 'NO' and $p.linker_input == 'UNPROVEN' and $p.legal_admission == 'NO' and $p.product_admission == 'NO' and $p.fleet_admission == 'NO') 'PREFLIGHT_SCOPE'
  let expected = ($l.cells | each {|x| {condition: $x.condition, profile: $x.profile, artifact_id: $x.artifact_id, artifact_zip_sha256: $x.artifact_zip_sha256, package_sha256: $x.deb_sha256}})
  require ($p.cells == $expected) 'PREFLIGHT_FOUR_TUPLES'
  require ((regular $deb) <= $l.max_deb_bytes and (sha $deb) == $c.deb_sha256) 'DEB_BYTES_DRIFT'
  let py = '
import hashlib,json,os,pathlib,re,subprocess,sys
condition,profile,elf_sha,archive_sha,notice_sha,cell_sha,deb_sha = sys.argv[1:]
def fail(code): raise SystemExit(code)
def cmd(args,stage):
    try:
        p=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=15,check=False)
    except subprocess.TimeoutExpired:
        fail(stage+"__TIMEOUT")
    if p.returncode: fail(stage+"__EXIT")
    if len(p.stdout)>65536: fail(stage+"__OUTPUT_CAP")
    try:
        return p.stdout.decode("utf-8","strict")
    except UnicodeDecodeError:
        fail(stage+"__DECODE")
def sha(path):
    h=hashlib.sha256()
    with open(path,"rb") as f:
        for block in iter(lambda:f.read(1024*1024),b""): h.update(block)
    return h.hexdigest()
status=cmd(["dpkg-query","-W","-f=${Status}\\t${Version}\\t${Architecture}\\n","auto-tundra"],"DPKG_QUERY_PACKAGE_STATUS")
if status!="install ok installed\t0.1.0\tamd64\n": fail("DPKG_STATUS")
prerequisites=[]
for name,stage in (("libwebkit2gtk-4.1-0","DPKG_QUERY_PREREQ_WEBKIT_STATUS"),("libgtk-3-0t64","DPKG_QUERY_PREREQ_GTK_STATUS")):
    row=cmd(["dpkg-query","-W","-f=${Status}\\t${Version}\\t${Architecture}\\n",name],stage).rstrip("\n").split("\t")
    if len(row)!=3 or row[0]!="install ok installed" or not row[1] or row[2]!="amd64": fail("PREREQ_STATUS_DRIFT")
    prerequisites.append({"package":name,"status":row[0],"version":row[1],"architecture":row[2]})
provided=cmd(["dpkg-query","-W","-f=${Provides}","libgtk-3-0t64"],"DPKG_QUERY_GTK_PROVIDER")
provided_tokens=[x.strip() for x in provided.split(",")]
matches=[x for x in provided_tokens if re.fullmatch(r"libgtk-3-0(?: \(= [^ )]+\))?",x)]
if len(matches)!=1: fail("PREREQ_PROVIDER_RELATION")
prerequisites[1]["provides_old_gtk"]="libgtk-3-0"
prerequisites[1]["provides_token"]=matches[0]
base="/usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime/"
paths={"elf":"/usr/bin/auto-tundra", "archive":base+"native-emscripten-libnoexit.a", "notice":base+"native-emscripten-LICENSE", "cell":base+"native-emscripten-libnoexit-cell.json"}
expected={"elf":elf_sha,"archive":archive_sha,"notice":notice_sha,"cell":cell_sha}
listed=set(cmd(["dpkg-query","-L","auto-tundra"],"DPKG_QUERY_PACKAGE_LIST").splitlines())
owner_stages={"elf":"DPKG_QUERY_OWNER_ELF","archive":"DPKG_QUERY_OWNER_ARCHIVE","notice":"DPKG_QUERY_OWNER_NOTICE","cell":"DPKG_QUERY_OWNER_CELL"}
for key,path in paths.items():
    if path not in listed: fail("DPKG_LIST_MISSING")
    current=pathlib.Path("/")
    for part in pathlib.PurePosixPath(path).parts[1:]:
        current=current/part
        if current.is_symlink(): fail("INSTALLED_SYMLINK")
    if not os.path.isfile(path) or os.path.islink(path) or os.path.realpath(path)!=path: fail("INSTALLED_PATH")
    owners=cmd(["dpkg-query","-S",path],owner_stages[key])
    if owners != "auto-tundra: "+path+"\n": fail("DPKG_OWNER")
    if sha(path)!=expected[key]: fail("INSTALLED_BYTES")
cell=json.loads(pathlib.Path(paths["cell"]).read_bytes())
if cell.get("condition")!=condition or cell.get("profile")!=profile or cell.get("archive_linker_input")!="UNPROVEN" or any(cell.get(k)!="NO" for k in ("legal_admission","product_admission","fleet_admission")): fail("INSTALLED_CELL_SCOPE")
print(json.dumps({"schema":"tundra-sdk-libnoexit-os-install-witness/v1","status":"DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY","condition":condition,"profile":profile,"package_sha256":deb_sha,"installed_elf_sha256":elf_sha,"installed_archive_sha256":archive_sha,"installed_notice_sha256":notice_sha,"installed_cell_sha256":cell_sha,"prerequisite_packages":prerequisites,"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
'
  let r = (^python3 -c $py $condition $profile $c.elf_sha256 $l.archive_sha256 $l.notice_sha256 $c.cell_resource_sha256 $c.deb_sha256 | complete)
  if $r.exit_code != 0 {
    let allowed = ['DPKG_STATUS' 'PREREQ_STATUS_DRIFT' 'PREREQ_PROVIDER_RELATION' 'DPKG_LIST_MISSING' 'INSTALLED_SYMLINK' 'INSTALLED_PATH' 'DPKG_OWNER' 'INSTALLED_BYTES' 'INSTALLED_CELL_SCOPE']
    let query_stages = ['DPKG_QUERY_PACKAGE_STATUS' 'DPKG_QUERY_PREREQ_WEBKIT_STATUS' 'DPKG_QUERY_PREREQ_GTK_STATUS' 'DPKG_QUERY_GTK_PROVIDER' 'DPKG_QUERY_PACKAGE_LIST' 'DPKG_QUERY_OWNER_ELF' 'DPKG_QUERY_OWNER_ARCHIVE' 'DPKG_QUERY_OWNER_NOTICE' 'DPKG_QUERY_OWNER_CELL']
    let query_kinds = ['TIMEOUT' 'EXIT' 'OUTPUT_CAP' 'DECODE']
    let observed = ($r.stderr | str trim)
    let parts = ($observed | split row '__')
    let query_ok = (($parts | length) == 2 and ($parts | first) in $query_stages and ($parts | last) in $query_kinds)
    let detail = if $query_ok { $parts | first } else if $observed in $allowed { $observed } else { 'UNCLASSIFIED' }
    let query_failure = if $query_ok { $parts | last } else { 'NOT_APPLICABLE' }
    $"schema=tundra-sdk-libnoexit-os-install-witness/v1\nstatus=REFUSED\nreason=OWNERSHIP_OR_BYTES_REFUSED\ndetail=($detail)\nquery_failure=($query_failure)\npackage_install_verified=NO\nbehavior_cases=NOT_RUN\nlinker_input=UNPROVEN\ngenerator_input_closure=UNPROVEN\nlegal_admission=NO\nproduct_admission=NO\nfleet_admission=NO\n" | save --force $refusal_path
    error make {msg: 'INSTALLED_WITNESS_REFUSED'}
  }
  require (($r.stdout | str length) <= $l.max_receipt_bytes) 'INSTALLED_WITNESS_REFUSED'
  let receipt = ($r.stdout | from json)
  require ($receipt.status == 'DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY' and $receipt.package_install_verified == 'YES_DISPOSABLE_RUNNER_ONLY' and $receipt.linker_input == 'UNPROVEN' and $receipt.legal_admission == 'NO' and $receipt.product_admission == 'NO' and $receipt.fleet_admission == 'NO') 'RECEIPT_SCOPE'
  $receipt | to json --raw | save $receipt_path
}
