#!/usr/bin/env nu

# Diagnostic only: authenticate PR151 packages, witness disposable installation,
# and validate the installed GUI corpus. No product or fleet admission.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def run_python [source: string, args: list<string>] {
  let result = (^python3 -c $source ...$args | complete)
  require ($result.exit_code == 0) ($result.stderr | str trim | default 'PYTHON_REFUSED')
  $result.stdout | from json
}
def ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-libnoexit-corpus-installed-behavior/v1' and $l.scope == 'FOUR_PR151_PACKAGES_DISPOSABLE_OS_INSTALL_AND_32_CASE_BEHAVIOR_ONLY') 'LEDGER_SCOPE'
  require ($l.repository == 'ryanmaclean/tundra' and $l.repository_id == 1164936674 and $l.source_head == '44b1c2008c40ca3d64695d4e44a4e586b5360a17' and $l.run_id == 40576558123 and $l.run_attempt == 1) 'LEDGER_RUN'
  require ($l.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and $l.harness_sha256 == '57480e689cccfa26e8527dba3f566c275bfbfad7cb2c43c4cb3dd85263284b8e') 'LEDGER_HARNESS'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'LEDGER_RESOURCES'
  require ($l.max_artifact_zip_bytes == 20971520 and $l.max_deb_bytes == 201326592 and $l.max_smoke_stdout_bytes == 1048576 and $l.max_smoke_stderr_bytes == 1048576 and $l.max_receipt_bytes == 131072) 'LEDGER_CAPS'
  require (($l.cells | length) == 4 and ($l.cells | each {|c| $"($c.condition)/($c.profile)"}) == ['image/default' 'image/sounds' 'clean/default' 'clean/sounds']) 'LEDGER_CELLS'
  require ($l.package_install_verified == 'PENDING_HOSTED_WITNESS' and $l.behavior_cases == 'PENDING_HOSTED_WITNESS' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_SCOPE'
  $l
}
def cell [l: record, condition: string, profile: string] {
  let rows = ($l.cells | where condition == $condition and profile == $profile)
  require (($rows | length) == 1) 'CELL_UNIQUE'
  $rows | first
}

def authenticate [ledger_path: string, run_path: string, root: string, condition: string, profile: string, output: string] {
  let l = (ledger $ledger_path)
  let c = (cell $l $condition $profile)
  let py = '
import hashlib,json,os,pathlib,stat,sys,zipfile
l=json.load(open(sys.argv[1])); run=json.load(open(sys.argv[2])); root=pathlib.Path(sys.argv[3]); condition,profile=sys.argv[4:6]
def fail(code): raise SystemExit(code)
def sha(b): return hashlib.sha256(b).hexdigest()
if not (run.get("id")==l["run_id"] and run.get("run_attempt")==l["run_attempt"] and run.get("head_sha")==l["source_head"] and run.get("event")=="workflow_dispatch" and run.get("status")=="completed" and run.get("conclusion")=="success" and run.get("repository",{}).get("id")==l["repository_id"]): fail("RUN_IDENTITY")
names=["package.deb","preflight.json","old-resource-ledger.json","corpus-source-ledger.json","source-receipt.json","corpus-package-identity.json","cleanup-proof.json","witness/cell-witness.json","witness/installed-archive","witness/installed-cell.json","witness/installed-notice"]
summary=[]
for c in l["cells"]:
  key=c["condition"]+"-"+c["profile"]; d=root/key
  a=json.loads((d/"artifact.json").read_bytes()); zpath=d/"artifact.zip"
  if not zpath.is_file() or zpath.is_symlink() or zpath.stat().st_size>l["max_artifact_zip_bytes"]: fail("ZIP_SIZE")
  zbytes=zpath.read_bytes()
  expected_name="duckdb-libnoexit-corpus-package-{}-{}-{}".format(l["run_id"],l["run_attempt"],key)
  wr=a.get("workflow_run",{})
  if not (a.get("id")==c["artifact_id"] and a.get("name")==expected_name and a.get("expired") is False and a.get("size_in_bytes")==len(zbytes) and a.get("digest")=="sha256:"+c["artifact_zip_sha256"] and wr.get("id")==l["run_id"] and wr.get("head_sha")==l["source_head"] and wr.get("repository_id")==l["repository_id"] and wr.get("head_repository_id")==l["repository_id"]): fail("ARTIFACT_API")
  if sha(zbytes)!=c["artifact_zip_sha256"]: fail("ZIP_HASH")
  with zipfile.ZipFile(zpath) as z:
    info=z.infolist()
    if sorted(x.filename for x in info)!=sorted(names) or len(info)!=len(names) or len(set(x.filename for x in info))!=len(names) or z.testzip() is not None: fail("ZIP_SHAPE")
    if any(not stat.S_ISREG(x.external_attr>>16) or x.flag_bits&1 or x.file_size>l["max_deb_bytes"] for x in info): fail("ZIP_TYPE_OR_CAP")
    if sum(x.file_size for x in info)>l["max_deb_bytes"]: fail("ZIP_EXPANDED_CAP")
    get=lambda n:z.read(n)
    identity=json.loads(get("corpus-package-identity.json")); witness=json.loads(get("witness/cell-witness.json")); source=json.loads(get("source-receipt.json")); cleanup=json.loads(get("cleanup-proof.json"))
    if not (identity.get("schema")=="tundra-libnoexit-corpus-package-identity/v1" and identity.get("status")=="NEW_CORPUS_PACKAGE_BYTES_DIAGNOSTIC_ONLY" and identity.get("condition")==c["condition"] and identity.get("profile")==c["profile"] and identity.get("gate_head")==l["source_head"] and identity.get("run_id")==l["run_id"] and identity.get("run_attempt")==l["run_attempt"] and identity.get("package_sha256")==c["deb_sha256"] and identity.get("extracted_elf_sha256")==c["elf_sha256"] and identity.get("cell_resource_sha256")==c["cell_resource_sha256"] and identity.get("archive_sha256")==l["archive_sha256"] and identity.get("notice_sha256")==l["notice_sha256"] and identity.get("corpus_source_sha256")==l["harness_sha256"] and identity.get("resource_witness_sha256")==sha(get("witness/cell-witness.json"))): fail("IDENTITY_JOIN")
    if not (identity.get("package_install_verified")=="NO" and identity.get("behavior_cases")=="NOT_RUN" and all(identity.get(k)=="NO" for k in ("legal_admission","product_admission","fleet_admission"))): fail("IDENTITY_SCOPE")
    if not (source.get("final_main_sha256")==l["harness_sha256"] and source.get("consumer_head")==l["consumer_head"] and witness.get("package_sha256")==c["deb_sha256"] and witness.get("cell_resource_sha256")==c["cell_resource_sha256"]): fail("SOURCE_WITNESS_JOIN")
    if not (cleanup.get("ephemeral_cleanup")=="PROVEN" and cleanup.get("status")=="BUILD_AND_DOWNLOAD_INPUTS_REMOVED"): fail("PRODUCER_CLEANUP")
    if sha(get("package.deb"))!=c["deb_sha256"] or len(get("package.deb"))!=identity.get("package_bytes"): fail("DEB_HASH")
    if sha(get("witness/installed-archive"))!=l["archive_sha256"] or sha(get("witness/installed-notice"))!=l["notice_sha256"] or sha(get("witness/installed-cell.json"))!=c["cell_resource_sha256"]: fail("RESOURCE_BYTES")
    if key==condition+"-"+profile: (root/"package.deb").write_bytes(get("package.deb"))
    summary.append({"condition":c["condition"],"profile":c["profile"],"artifact_id":c["artifact_id"],"artifact_zip_sha256":c["artifact_zip_sha256"],"package_sha256":c["deb_sha256"]})
print(json.dumps({"schema":l["schema"],"status":"FOUR_PR151_PACKAGES_AUTHENTICATED_ONLY","run_id":l["run_id"],"source_head":l["source_head"],"cells":summary,"package_install_verified":"NO","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $run_path $root $condition $profile])
  require ($receipt.status == 'FOUR_PR151_PACKAGES_AUTHENTICATED_ONLY' and ($receipt.cells | length) == 4) 'AUTHENTICATION_SCOPE'
  $receipt | to json --raw | save $output
}

def installed [ledger_path: string, condition: string, profile: string, deb: string, output: string] {
  let l = (ledger $ledger_path)
  let c = (cell $l $condition $profile)
  let py = '
import hashlib,json,os,pathlib,re,subprocess,sys
l=json.load(open(sys.argv[1])); c=next(x for x in l["cells"] if x["condition"]==sys.argv[2] and x["profile"]==sys.argv[3]); deb=pathlib.Path(sys.argv[4])
def fail(code): raise SystemExit(code)
def sha(p): return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def cmd(args,code,cap=131072):
  try: r=subprocess.run(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15)
  except subprocess.TimeoutExpired: fail(code+"_TIMEOUT")
  if r.returncode or len(r.stdout)>cap or len(r.stderr)>cap: fail(code)
  return r.stdout.decode("utf-8","strict")
if not deb.is_file() or deb.is_symlink() or deb.stat().st_size>l["max_deb_bytes"] or sha(deb)!=c["deb_sha256"]: fail("DEB_DRIFT")
if cmd(["dpkg-query","-W","-f=${Status}\\t${Version}\\t${Architecture}\\n","auto-tundra"],"PACKAGE_STATUS")!="install ok installed\t0.1.0\tamd64\n": fail("PACKAGE_STATUS_DRIFT")
prereqs=[]
for name in ("libwebkit2gtk-4.1-0","libgtk-3-0t64"):
  row=cmd(["dpkg-query","-W","-f=${Status}\\t${Version}\\t${Architecture}\\n",name],"PREREQ_STATUS").strip().split("\t")
  if len(row)!=3 or row[0]!="install ok installed" or row[2]!="amd64": fail("PREREQ_STATUS_DRIFT")
  prereqs.append({"package":name,"version":row[1],"architecture":row[2]})
provided=cmd(["dpkg-query","-W","-f=${Provides}","libgtk-3-0t64"],"GTK_PROVIDES")
if len([x for x in provided.split(",") if re.fullmatch(r"libgtk-3-0(?: \(= [^ )]+\))?",x.strip())])!=1: fail("GTK_PROVIDER_DRIFT")
base="/usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime/"
paths={"elf":"/usr/bin/auto-tundra","archive":base+"native-emscripten-libnoexit.a","notice":base+"native-emscripten-LICENSE","cell":base+"native-emscripten-libnoexit-cell.json"}
expected={"elf":c["elf_sha256"],"archive":l["archive_sha256"],"notice":l["notice_sha256"],"cell":c["cell_resource_sha256"]}
listed=set(cmd(["dpkg-query","-L","auto-tundra"],"PACKAGE_LIST").splitlines())
for kind,path in paths.items():
  if path not in listed: fail("PACKAGE_LIST_MISSING")
  current=pathlib.Path("/")
  for part in pathlib.PurePosixPath(path).parts[1:]:
    current=current/part
    if current.is_symlink(): fail("INSTALLED_SYMLINK")
  if not os.path.isfile(path) or os.path.islink(path) or os.path.realpath(path)!=path: fail("INSTALLED_PATH")
  if cmd(["dpkg-query","-S",path],"PACKAGE_OWNER")!="auto-tundra: "+path+"\n": fail("PACKAGE_OWNER_DRIFT")
  if sha(path)!=expected[kind]: fail("INSTALLED_BYTES_DRIFT")
cell=json.loads(pathlib.Path(paths["cell"]).read_bytes())
if cell.get("condition")!=c["condition"] or cell.get("profile")!=c["profile"] or cell.get("archive_linker_input")!="UNPROVEN": fail("INSTALLED_CELL_SCOPE")
print(json.dumps({"schema":l["schema"],"status":"DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY","condition":c["condition"],"profile":c["profile"],"package_sha256":c["deb_sha256"],"installed_elf_sha256":c["elf_sha256"],"installed_archive_sha256":l["archive_sha256"],"installed_notice_sha256":l["notice_sha256"],"installed_cell_sha256":c["cell_resource_sha256"],"prerequisites":prereqs,"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $condition $profile $deb])
  require ($receipt.status == 'DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY' and $receipt.installed_elf_sha256 == $c.elf_sha256) 'INSTALLED_SCOPE'
  $receipt | to json --raw | save $output
}

def smoke [ledger_path: string, condition: string, profile: string, install_receipt: string, stdout_path: string, stderr_path: string, exit_code: int, output: string] {
  let l = (ledger $ledger_path)
  let c = (cell $l $condition $profile)
  let py = '
import hashlib,json,pathlib,sys
l=json.load(open(sys.argv[1])); c=next(x for x in l["cells"] if x["condition"]==sys.argv[2] and x["profile"]==sys.argv[3]); i=json.load(open(sys.argv[4])); stdout=pathlib.Path(sys.argv[5]); stderr=pathlib.Path(sys.argv[6]); rc=int(sys.argv[7])
def fail(code): raise SystemExit(code)
if rc!=0 or stdout.stat().st_size>l["max_smoke_stdout_bytes"] or stderr.stat().st_size>l["max_smoke_stderr_bytes"]: fail("PROCESS_OR_OUTPUT_CAP")
lines=[x[len("TUNDRA_DUCKDB_SMOKE:"):] for x in stdout.read_text(errors="replace").splitlines() if x.startswith("TUNDRA_DUCKDB_SMOKE:")]
if len(lines)!=1 or len(lines[0])>l["max_receipt_bytes"]: fail("SMOKE_MARKER")
r=json.loads(lines[0]); result=r.get("result",{})
if not (i.get("schema")==l["schema"] and i.get("status")=="DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY" and i.get("condition")==c["condition"] and i.get("profile")==c["profile"] and i.get("package_sha256")==c["deb_sha256"] and i.get("installed_elf_sha256")==c["elf_sha256"] and i.get("installed_archive_sha256")==l["archive_sha256"] and i.get("installed_notice_sha256")==l["notice_sha256"] and i.get("installed_cell_sha256")==c["cell_resource_sha256"] and i.get("package_install_verified")=="YES_DISPOSABLE_RUNNER_ONLY" and i.get("behavior_cases")=="NOT_RUN" and i.get("linker_input")=="UNPROVEN" and i.get("generator_input_closure")=="UNPROVEN" and all(i.get(k)=="NO" for k in ("legal_admission","product_admission","fleet_admission")) and [x.get("package") for x in i.get("prerequisites",[])]==["libwebkit2gtk-4.1-0","libgtk-3-0t64"]): fail("INSTALLED_TUPLE")
if not (r.get("schema")=="tundra-duckdb-webkit-smoke/v1" and r.get("head_sha")==l["consumer_head"] and r.get("profile")==c["profile"] and r.get("installed_elf_sha256")==c["elf_sha256"] and r.get("requested_variant")=="eh" and r.get("ok") is True and result.get("variant")=="eh" and result.get("ok") is True and result.get("exceptionOk") is True and result.get("cspDenied") is True and result.get("cspViolations")==[] and result.get("localRuntime") is True and result.get("remoteRequests")==0): fail("SMOKE_SCOPE")
s=json.loads(result["semanticObserved"])
if not (s.get("sameConnection") is True and s.get("recovery") is True and s.get("targetedPathsOk") is True and len(s.get("cases",[]))==9): fail("NINE_SEMANTIC_PATHS")
corpus=json.loads(result["corpusObserved"])
ids="bridge-create bridge-json-insert bridge-empty-insert bridge-persist-null-utf8 bridge-macro-create bridge-macro-query bridge-parser-error bridge-parser-recovery bridge-catalog-error bridge-catalog-recovery bridge-binder-error bridge-conversion-error bridge-after-errors bridge-arrow-json-types bridge-large-bigint-string installed-local-pair-urls installed-local-pair-bytes direct-arrow-schema direct-csv-register direct-csv-scan direct-csv-insert direct-json-register direct-json-scan direct-json-insert direct-parquet-copy direct-parquet-buffer direct-parquet-reregister direct-parquet-scan direct-file-error direct-after-error direct-arrow-invalid-ipc direct-after-arrow-error".split()
cases=corpus.get("cases",[])
if not (corpus.get("schema")=="tundra-installed-eh-corpus/v1" and corpus.get("directConnection")=="SEPARATE_SAME_INSTALLED_PAIR" and len(cases)==32 and sorted(x.get("id") for x in cases)==sorted(ids) and all(x.get("passed") is True and x.get("skipped",False) is False and x.get("unsupported",False) is False for x in cases) and corpus.get("passed") is True and corpus.get("skipped")==[] and sorted(corpus.get("unsupported",[]))==sorted(["arrow-positive-ipc","multibatch-stream","upstream-full-suite"])): fail("CORPUS_32_SCOPE")
digest=lambda x:hashlib.sha256(json.dumps(x,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()
out={"schema":l["schema"],"status":"INSTALLED_32_CASE_CORPUS_DIAGNOSTIC_ONLY","smoke_schema":r["schema"],"source_head":l["source_head"],"consumer_head":l["consumer_head"],"harness_sha256":l["harness_sha256"],"semantic_paths":9,"semantic_sha256":digest(s),"corpus_schema":corpus["schema"],"corpus_passed":True,"corpus_cases":[{"id":x["id"],"passed":True,"case_sha256":digest(x)} for x in cases],"unsupported_cases":sorted(corpus["unsupported"]),"remote_requests":0,"local_runtime":True,"csp_denied":True,"csp_violations":0,"diagnostic_install":{"package_sha256":c["deb_sha256"],"installed_elf_sha256":c["elf_sha256"],"installed_archive_sha256":l["archive_sha256"],"installed_notice_sha256":l["notice_sha256"],"installed_cell_sha256":c["cell_resource_sha256"],"condition":c["condition"],"profile":c["profile"],"producer_run_id":l["run_id"],"producer_artifact_id":c["artifact_id"],"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"32_PASSED_DISPOSABLE_RUNNER_ONLY","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}}
print(json.dumps(out,sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $condition $profile $install_receipt $stdout_path $stderr_path ($exit_code | into string)])
  require (($receipt | to json --raw | str length) <= $l.max_receipt_bytes) 'RECEIPT_CAP'
  $receipt | to json --raw | save $output
}

def main [mode: string, ...args: string] {
  match $mode {
    'authenticate' => { require (($args | length) == 6) 'ARGS'; authenticate $args.0 $args.1 $args.2 $args.3 $args.4 $args.5 }
    'installed' => { require (($args | length) == 5) 'ARGS'; installed $args.0 $args.1 $args.2 $args.3 $args.4 }
    'smoke' => { require (($args | length) == 8) 'ARGS'; smoke $args.0 $args.1 $args.2 $args.3 $args.4 $args.5 ($args.6 | into int) $args.7 }
    _ => { error make {msg: 'MODE'} }
  }
}
