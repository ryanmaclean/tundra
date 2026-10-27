#!/usr/bin/env nu

# Diagnostic only: authenticate PR153 packages, witness disposable installation,
# and validate the installed GUI corpus. No product or fleet admission.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def run_python [source: string, args: list<string>] {
  let result = (^python3 -c $source ...$args | complete)
  require ($result.exit_code == 0) ($result.stderr | str trim | default 'PYTHON_REFUSED')
  $result.stdout | from json
}
def ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-libnoexit-new-runtime-installed-behavior/v1' and $l.scope == 'FOUR_PR153_NEW_RUNTIME_PACKAGES_DISPOSABLE_OS_INSTALL_AND_32_CASE_BEHAVIOR_ONLY') 'LEDGER_SCOPE'
  require ($l.repository == 'ryanmaclean/tundra' and $l.repository_id == 1164936674 and $l.source_head == '1cf16aa6c5c0fa6265e06ccc03ad9ef5e3f35875' and $l.run_id == 40588647258 and $l.run_attempt == 1) 'LEDGER_RUN'
  require ($l.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and $l.harness_sha256 == '57480e689cccfa26e8527dba3f566c275bfbfad7cb2c43c4cb3dd85263284b8e') 'LEDGER_HARNESS'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'LEDGER_RESOURCES'
  require ($l.source_ledger_sha256 == '4c4c204139621c4ba1dda01ee4eff5590cf44d02051835f1e79bbbc8f84d24a8' and $l.corpus_source_ledger_sha256 == '80b4c8e2a6f42f3d8f8bab45d8fb65c7ed1639596b363d80216ee76dad4f71cf' and $l.pr117_artifact_id == 12607318747 and $l.pr136_artifact_id == 12705817192) 'LEDGER_PROVENANCE'
  require ($l.assets.'duckdb-browser.mjs' == '8ccf3fe3c6bd87d5c3e397f01d40cc11ea6373b016a9cf5e954d39ee22c3aa49' and $l.assets.'duckdb-browser-eh.worker.js' == 'feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4' and $l.assets.'duckdb-browser-mvp.worker.js' == 'd7920b73954d99ffa59175db35fe24e7936f8a003144386686b53c853c46eaef' and $l.assets.'duckdb-mvp.wasm' == '7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791' and $l.assets.'duckdb-eh.wasm'.image == 'ac2022967687136ec63c8917d77842f3c01f69291d2d024626e42a34d17953b8' and $l.assets.'duckdb-eh.wasm'.clean == 'd267671286a6f0718a6fe1d712cd209bda84ef53f5984d0303defeb55c9f5d99') 'LEDGER_FIVE_ASSETS'
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
import hashlib,json,pathlib,stat,sys,zipfile
l=json.load(open(sys.argv[1])); run=json.load(open(sys.argv[2])); root=pathlib.Path(sys.argv[3]); condition,profile=sys.argv[4:6]
def fail(code): raise SystemExit(code)
def sha(b): return hashlib.sha256(b).hexdigest()
def parsed(b): return json.loads(b)
if not (run.get("id")==l["run_id"] and run.get("run_attempt")==l["run_attempt"] and run.get("head_sha")==l["source_head"] and run.get("event")=="workflow_dispatch" and run.get("status")=="completed" and run.get("conclusion")=="success" and run.get("repository",{}).get("id")==l["repository_id"] and run.get("head_repository",{}).get("id")==l["repository_id"]): fail("RUN_IDENTITY")
names=["package.deb","preflight.json","installed-selected-bill.json","corpus-source-ledger.json","source-receipt.json","corpus-package-identity.json","cleanup-proof.json","witness/cell-witness.json","witness/installed-archive","witness/installed-cell.json","witness/installed-notice"]
summary=[]
for c in l["cells"]:
  key=c["condition"]+"-"+c["profile"]; d=root/key; a=parsed((d/"artifact.json").read_bytes()); zpath=d/"artifact.zip"
  if not zpath.is_file() or zpath.is_symlink() or zpath.stat().st_size>l["max_artifact_zip_bytes"]: fail("ZIP_SIZE")
  zbytes=zpath.read_bytes(); wr=a.get("workflow_run",{}); expected_name="duckdb-libnoexit-new-runtime-package-{}-{}-{}".format(l["run_id"],l["run_attempt"],key)
  if not (a.get("id")==c["artifact_id"] and a.get("name")==expected_name and a.get("expired") is False and a.get("size_in_bytes")==len(zbytes) and a.get("digest")=="sha256:"+c["artifact_zip_sha256"] and wr.get("id")==l["run_id"] and wr.get("head_sha")==l["source_head"] and wr.get("repository_id")==l["repository_id"] and wr.get("head_repository_id")==l["repository_id"]): fail("ARTIFACT_API")
  if sha(zbytes)!=c["artifact_zip_sha256"]: fail("ZIP_HASH")
  with zipfile.ZipFile(zpath) as z:
    info=z.infolist(); listed=[x.filename for x in info]
    if sorted(listed)!=sorted(names) or len(info)!=len(names) or len(set(listed))!=len(names) or z.testzip() is not None: fail("ZIP_SHAPE")
    if any(not stat.S_ISREG(x.external_attr>>16) or x.flag_bits&1 or x.file_size>l["max_deb_bytes"] for x in info) or sum(x.file_size for x in info)>l["max_deb_bytes"]: fail("ZIP_TYPE_OR_CAP")
    get=lambda n:z.read(n)
    identity=parsed(get("corpus-package-identity.json")); witness=parsed(get("witness/cell-witness.json")); source=parsed(get("source-receipt.json")); preflight=parsed(get("preflight.json")); cleanup=parsed(get("cleanup-proof.json")); bill=parsed(get("installed-selected-bill.json")); cell=parsed(get("witness/installed-cell.json"))
    assets={k:(v[c["condition"]] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
    required={"schema":"tundra-libnoexit-new-runtime-package-identity/v1","status":"NEW_RUNTIME_PACKAGE_BYTES_DIAGNOSTIC_ONLY","condition":c["condition"],"profile":c["profile"],"gate_head":l["source_head"],"run_id":l["run_id"],"run_attempt":l["run_attempt"],"consumer_head":l["consumer_head"],"pr117_artifact_id":l["pr117_artifact_id"],"pr136_artifact_id":l["pr136_artifact_id"],"pr137_artifact_id":c["pr137_artifact_id"],"pr137_zip_sha256":c["pr137_zip_sha256"],"source_ledger_sha256":l["source_ledger_sha256"],"corpus_source_ledger_sha256":l["corpus_source_ledger_sha256"],"source_receipt_sha256":sha(get("source-receipt.json")),"preflight_sha256":sha(get("preflight.json")),"corpus_source_sha256":l["harness_sha256"],"selected_bill_sha256":c["bill_sha256"],"package_sha256":c["deb_sha256"],"package_bytes":len(get("package.deb")),"extracted_elf_sha256":c["elf_sha256"],"cell_resource_sha256":c["cell_resource_sha256"],"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"assets":assets,"notice_text_count":37,"selected_notice_joins":71,"unresolved_selected_keys":35,"package_install_verified":"NO","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","selected_graph_exhaustive":"NO","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
    if identity!=required: fail("IDENTITY_TUPLE")
    expected_witness={"schema":"tundra-libnoexit-new-runtime-package-witness/v1","condition":c["condition"],"profile":c["profile"],"package_sha256":c["deb_sha256"],"installed_elf_sha256":c["elf_sha256"],"selected_bill_sha256":c["bill_sha256"],"cell_resource_sha256":c["cell_resource_sha256"],"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"source_dist_embedded_assets":"FIVE_IDENTICAL_HASHES","package_install_verified":"NO","behavior_cases":"NOT_RUN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
    if witness!=expected_witness: fail("WITNESS_TUPLE")
    if sha(get("package.deb"))!=c["deb_sha256"] or sha(get("corpus-source-ledger.json"))!=l["corpus_source_ledger_sha256"] or sha(get("installed-selected-bill.json"))!=c["bill_sha256"] or sha(get("witness/installed-cell.json"))!=c["cell_resource_sha256"] or sha(get("witness/installed-archive"))!=l["archive_sha256"] or sha(get("witness/installed-notice"))!=l["notice_sha256"]: fail("RETAINED_MEMBER_HASH")
    if not (source.get("schema")=="tundra-libnoexit-corpus-package-source/v1" and source.get("status")=="EXACT_PR137_SOURCE_TRANSFORMATION_ONLY" and source.get("final_main_sha256")==l["harness_sha256"] and preflight.get("status")=="PRODUCER_BYTES_AUTHENTICATED_ONLY" and preflight.get("legal_admission")=="NO" and cleanup=={"schema":"tundra-libnoexit-new-runtime-package-cleanup/v1","status":"BUILD_AND_DOWNLOAD_INPUTS_REMOVED","ephemeral_cleanup":"PROVEN","package_install_verified":"NO","behavior_cases":"NOT_RUN","product_admission":"NO","fleet_admission":"NO"}): fail("SOURCE_CLEANUP_SCOPE")
    if not (bill.get("schema")=="tundra-duckdb-installed-json-package-selected-bill/v5" and bill.get("condition")==c["condition"] and bill.get("profile")==c["profile"] and bill.get("producer_run")==40434996466 and bill.get("producer_bill_sha256")=="93c4637dcd0cf4527814267b38a242cc626ad64f92df0a9a52928d1edc036c0f" and bill.get("condition_output",{}).get("condition")==c["condition"] and bill.get("condition_output",{}).get("wasm_sha256")==assets["duckdb-eh.wasm"] and bill.get("condition_output",{}).get("js_sha256")=="fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3" and bill.get("rebuilt_worker_sha256")==assets["duckdb-browser-eh.worker.js"] and bill.get("legal_admission")=="NO" and len(bill.get("texts",[]))==37 and len(bill.get("selected_notice_joins",[]))==71 and len(bill.get("selected_unresolved_keys",[]))==35): fail("V5_BILL_SCOPE")
    if {x["path"].split("/")[-1]:x["sha256"] for x in bill["assets"]}!=assets or len(bill["assets"])!=5: fail("FIVE_ASSET_BILL")
    if not (cell.get("schema")=="tundra-libnoexit-new-runtime-resource-cell/v1" and cell.get("condition")==c["condition"] and cell.get("profile")==c["profile"] and cell.get("pr137_artifact_id")==c["pr137_artifact_id"] and cell.get("pr137_selected_bill_sha256")==c["bill_sha256"] and cell.get("assets")==assets and cell.get("archive_sha256")==l["archive_sha256"] and cell.get("notice_sha256")==l["notice_sha256"] and cell.get("legal_admission")=="NO"): fail("CELL_PROVENANCE")
    if key==condition+"-"+profile: (root/"package.deb").write_bytes(get("package.deb"))
    summary.append({"condition":c["condition"],"profile":c["profile"],"artifact_id":c["artifact_id"],"artifact_zip_sha256":c["artifact_zip_sha256"],"package_sha256":c["deb_sha256"],"bill_sha256":c["bill_sha256"]})
print(json.dumps({"schema":l["schema"],"status":"FOUR_PR153_PACKAGES_AUTHENTICATED_ONLY","run_id":l["run_id"],"source_head":l["source_head"],"cells":summary,"package_install_verified":"NO","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $run_path $root $condition $profile])
  require ($receipt.status == 'FOUR_PR153_PACKAGES_AUTHENTICATED_ONLY' and ($receipt.cells | length) == 4) 'AUTHENTICATION_SCOPE'
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
paths={"elf":"/usr/bin/auto-tundra","archive":base+"native-emscripten-libnoexit.a","notice":base+"native-emscripten-LICENSE","cell":base+"native-emscripten-libnoexit-cell.json","bill":"/usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime-selected-bill.json"}
expected={"elf":c["elf_sha256"],"archive":l["archive_sha256"],"notice":l["notice_sha256"],"cell":c["cell_resource_sha256"],"bill":c["bill_sha256"]}
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
assets={k:(v[c["condition"]] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
cell=json.loads(pathlib.Path(paths["cell"]).read_bytes()); bill=json.loads(pathlib.Path(paths["bill"]).read_bytes())
if not (cell.get("schema")=="tundra-libnoexit-new-runtime-resource-cell/v1" and cell.get("condition")==c["condition"] and cell.get("profile")==c["profile"] and cell.get("assets")==assets and cell.get("pr137_artifact_id")==c["pr137_artifact_id"] and cell.get("pr137_selected_bill_sha256")==c["bill_sha256"] and cell.get("archive_linker_input")=="UNPROVEN" and cell.get("legal_admission")=="NO"): fail("INSTALLED_CELL_SCOPE")
if not (bill.get("schema")=="tundra-duckdb-installed-json-package-selected-bill/v5" and bill.get("condition")==c["condition"] and bill.get("profile")==c["profile"] and bill.get("condition_output",{}).get("condition")==c["condition"] and bill.get("condition_output",{}).get("wasm_sha256")==assets["duckdb-eh.wasm"] and bill.get("condition_output",{}).get("js_sha256")=="fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3" and bill.get("rebuilt_worker_sha256")==assets["duckdb-browser-eh.worker.js"] and bill.get("legal_admission")=="NO" and len(bill.get("texts",[]))==37 and len(bill.get("selected_notice_joins",[]))==71 and len(bill.get("selected_unresolved_keys",[]))==35 and len(bill.get("assets",[]))==5 and {x["path"].split("/")[-1]:x["sha256"] for x in bill["assets"]}==assets): fail("INSTALLED_BILL_SCOPE")
texts=bill["texts"]
if len({x["file"] for x in texts})!=37: fail("INSTALLED_NOTICE_SET")
for row in texts:
  path="/usr/lib/auto-tundra/"+row["file"]
  if path not in listed or not path.startswith(base) or pathlib.PurePosixPath(path).name in (".",".."): fail("INSTALLED_NOTICE_PATH")
  current=pathlib.Path("/")
  for part in pathlib.PurePosixPath(path).parts[1:]:
    current=current/part
    if current.is_symlink(): fail("INSTALLED_NOTICE_SYMLINK")
  if not os.path.isfile(path) or os.path.realpath(path)!=path or cmd(["dpkg-query","-S",path],"NOTICE_OWNER")!="auto-tundra: "+path+"\n" or sha(path)!=row["sha256"]: fail("INSTALLED_NOTICE_BYTES")
print(json.dumps({"schema":l["schema"],"status":"DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY","condition":c["condition"],"profile":c["profile"],"package_sha256":c["deb_sha256"],"installed_elf_sha256":c["elf_sha256"],"installed_archive_sha256":l["archive_sha256"],"installed_notice_sha256":l["notice_sha256"],"installed_cell_sha256":c["cell_resource_sha256"],"installed_bill_sha256":c["bill_sha256"],"installed_notice_count":37,"assets":assets,"prerequisites":prereqs,"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
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
if not isinstance(r,dict) or set(r)!={"head_sha","installed_elf_sha256","ok","profile","requested_variant","result","schema"}: fail("MARKER_FIELDS")
if not isinstance(result,dict) or set(result)!={"absentIsNull","aggregateObserved","aggregateOk","answer","corpusObserved","cspDenied","cspViolations","exceptionErrorClass","exceptionErrorFragment","exceptionGate","exceptionOk","localRuntime","ok","recoveryOk","recoveryRow","rejectedObserved","rejectedShape","rejectionProbes","remoteRequests","rowCount","semanticObserved","smokeError","variant"}: fail("RESULT_FIELDS")
assets={k:(v[c["condition"]] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
if not (i.get("schema")==l["schema"] and i.get("status")=="DISPOSABLE_DPKG_OWNERSHIP_AND_BYTES_ONLY" and i.get("condition")==c["condition"] and i.get("profile")==c["profile"] and i.get("package_sha256")==c["deb_sha256"] and i.get("installed_elf_sha256")==c["elf_sha256"] and i.get("installed_archive_sha256")==l["archive_sha256"] and i.get("installed_notice_sha256")==l["notice_sha256"] and i.get("installed_cell_sha256")==c["cell_resource_sha256"] and i.get("installed_bill_sha256")==c["bill_sha256"] and i.get("installed_notice_count")==37 and i.get("assets")==assets and i.get("package_install_verified")=="YES_DISPOSABLE_RUNNER_ONLY" and i.get("behavior_cases")=="NOT_RUN" and i.get("linker_input")=="UNPROVEN" and i.get("generator_input_closure")=="UNPROVEN" and all(i.get(k)=="NO" for k in ("legal_admission","product_admission","fleet_admission")) and [x.get("package") for x in i.get("prerequisites",[])]==["libwebkit2gtk-4.1-0","libgtk-3-0t64"]): fail("INSTALLED_TUPLE")
if not (r.get("schema")=="tundra-duckdb-webkit-smoke/v1" and r.get("head_sha")==l["consumer_head"] and r.get("profile")==c["profile"] and r.get("installed_elf_sha256")==c["elf_sha256"] and r.get("requested_variant")=="eh" and r.get("ok") is True and result.get("variant")=="eh" and result.get("ok") is True and result.get("exceptionOk") is True and result.get("cspDenied") is True and result.get("cspViolations")==[] and result.get("localRuntime") is True and result.get("remoteRequests")==0): fail("SMOKE_SCOPE")
s=json.loads(result["semanticObserved"])
if not isinstance(s,dict) or set(s)!={"cases","recovery","sameConnection","targetedPathsOk"} or not isinstance(s.get("cases"),list) or any(not isinstance(x,dict) or set(x)!={"errorClass","marker","name","rows","shape"} for x in s["cases"]): fail("SEMANTIC_FIELDS")
if not (s.get("sameConnection") is True and s.get("recovery") is True and s.get("targetedPathsOk") is True and len(s.get("cases",[]))==9): fail("NINE_SEMANTIC_PATHS")
try: corpus=json.loads(result["corpusObserved"])
except (KeyError,TypeError,ValueError): fail("CORPUS_JSON")
if not isinstance(corpus,dict): fail("CORPUS_TYPE")
if set(corpus)!={"cases","conversionDiagnostics","directConnection","fixtures","passed","schema","selected","skipped","unsupported"}: fail("CORPUS_FIELDS")
ids="bridge-create bridge-json-insert bridge-empty-insert bridge-persist-null-utf8 bridge-macro-create bridge-macro-query bridge-parser-error bridge-parser-recovery bridge-catalog-error bridge-catalog-recovery bridge-binder-error bridge-conversion-error bridge-after-errors bridge-arrow-json-types bridge-large-bigint-string installed-local-pair-urls installed-local-pair-bytes direct-arrow-schema direct-csv-register direct-csv-scan direct-csv-insert direct-json-register direct-json-scan direct-json-insert direct-parquet-copy direct-parquet-buffer direct-parquet-reregister direct-parquet-scan direct-file-error direct-after-error direct-arrow-invalid-ipc direct-after-arrow-error".split()
cases=corpus.get("cases",[])
if corpus.get("schema")!="tundra-installed-eh-corpus/v1": fail("CORPUS_SCHEMA")
if corpus.get("directConnection")!="SEPARATE_SAME_INSTALLED_PAIR": fail("CORPUS_DIRECT_CONNECTION")
if not isinstance(cases,list): fail("CORPUS_CASES_TYPE")
if len(cases)!=32: fail("CORPUS_CASE_COUNT")
if any(not isinstance(x,dict) or not isinstance(x.get("id"),str) for x in cases): fail("CORPUS_CASE_SHAPE")
case_fields={"absenceClass","absent","canonicalBytes","canonicalSha256","errorClass","fixtureBytes","fixtureSha256","id","marker","moduleUrl","passed","phase","recovered","rejectionStage","responseDigest","responseLength","responseShape","wasmUrl","workerUrl"}
if any(not set(x)<=case_fields for x in cases): fail("CORPUS_CASE_FIELDS")
observed=[x["id"] for x in cases]
if len(set(observed))!=32: fail("CORPUS_CASE_ID_DUPLICATE")
if sorted(observed)!=sorted(ids): fail("CORPUS_CASE_ID_SET")
expected_phase={x:("bridge" if x.startswith("bridge-") else "provenance" if x in ("installed-local-pair-urls","installed-local-pair-bytes") else "direct") for x in ids}
if any(x.get("phase")!=expected_phase[x["id"]] for x in cases): fail("CORPUS_CASE_PHASE")
if any(type(x.get("passed")) is not bool for x in cases): fail("CORPUS_CASE_VERDICT_TYPE")
failed=[x["id"] for x in cases if x["passed"] is False]
if failed: fail("CORPUS_CASE_FAILED_IDS="+",".join(sorted(failed)))
if any(x.get("skipped",False) is not False for x in cases): fail("CORPUS_CASE_SKIPPED")
if any(x.get("unsupported",False) is not False for x in cases): fail("CORPUS_CASE_UNSUPPORTED")
if corpus.get("passed") is not True: fail("CORPUS_AGGREGATE_VERDICT")
if corpus.get("skipped")!=[]: fail("CORPUS_SKIPPED_LIST")
if not isinstance(corpus.get("unsupported"),list) or any(not isinstance(x,str) for x in corpus["unsupported"]) or sorted(corpus["unsupported"])!=sorted(["arrow-positive-ipc","multibatch-stream","upstream-full-suite"]): fail("CORPUS_UNSUPPORTED_LIST")
digest=lambda x:hashlib.sha256(json.dumps(x,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()).hexdigest()
out={"schema":l["schema"],"status":"INSTALLED_32_CASE_CORPUS_DIAGNOSTIC_ONLY","smoke_schema":r["schema"],"source_head":l["source_head"],"consumer_head":l["consumer_head"],"harness_sha256":l["harness_sha256"],"semantic_paths":9,"semantic_sha256":digest(s),"corpus_schema":corpus["schema"],"corpus_passed":True,"corpus_cases":[{"id":x["id"],"phase":expected_phase[x["id"]],"passed":True,"case_sha256":digest(x)} for x in cases],"unsupported_cases":sorted(corpus["unsupported"]),"remote_requests":0,"local_runtime":True,"csp_denied":True,"csp_violations":0,"diagnostic_install":{"package_sha256":c["deb_sha256"],"installed_elf_sha256":c["elf_sha256"],"installed_archive_sha256":l["archive_sha256"],"installed_notice_sha256":l["notice_sha256"],"installed_cell_sha256":c["cell_resource_sha256"],"installed_bill_sha256":c["bill_sha256"],"installed_notice_count":37,"assets":assets,"condition":c["condition"],"profile":c["profile"],"producer_run_id":l["run_id"],"producer_artifact_id":c["artifact_id"],"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"32_PASSED_DISPOSABLE_RUNNER_ONLY","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}}
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
