#!/usr/bin/env nu

# PR117 custody + PR136 condition + PR137 installed-trial inputs. Diagnostic only.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def run_python [source: string, args: list<string>] {
  let r = (^python3 -c $source ...$args | complete)
  require ($r.exit_code == 0) ($r.stderr | str trim | default 'PYTHON_REFUSED')
  $r.stdout | from json
}
def checked [path: string, condition: string, profile: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-libnoexit-new-runtime-package-source/v1' and $l.scope == 'PR117_PR136_PR137_FOUR_DIAGNOSTIC_PACKAGES_ONLY') 'LEDGER_SCOPE'
  require ($l.repository_id == 1164936674 and $l.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and $l.pr117_run == 40217360882 and $l.pr117_artifact_id == 12607318747 and $l.pr117_custody_commit == '1d77df7776dca44731ae8267092e480a78f9f735') 'PR117_PIN'
  require ($l.pr136_run == 40434996466 and $l.pr136_artifact_id == 12705817192 and $l.pr136_bill_sha256 == '93c4637dcd0cf4527814267b38a242cc626ad64f92df0a9a52928d1edc036c0f' and $l.pr137_run == 40442338363 and $l.pr137_attempt == 1) 'PRODUCER_PIN'
  require ($l.corpus_source_ledger_sha256 == '80b4c8e2a6f42f3d8f8bab45d8fb65c7ed1639596b363d80216ee76dad4f71cf' and $l.final_main_sha256 == '57480e689cccfa26e8527dba3f566c275bfbfad7cb2c43c4cb3dd85263284b8e') 'CORPUS_PIN'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'CONTROLLED_PIN'
  require ($l.max_producer_zip_bytes == 67108864 and $l.max_trial_zip_bytes == 8388608 and $l.max_package_bytes == 201326592) 'CAP_PIN'
  require (($l.cells | length) == 4 and ($l.cells | each {|c| $"($c.condition)/($c.profile)"}) == ['image/default' 'image/sounds' 'clean/default' 'clean/sounds']) 'CELL_SET'
  require ($l.package_install_verified == 'NO' and $l.behavior_cases == 'NOT_RUN' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'ADMISSION_SCOPE'
  let rows = ($l.cells | where condition == $condition and profile == $profile)
  require (($rows | length) == 1) 'CELL_UNIQUE'
  $l
}

def stage [ledger_path: string, condition: string, profile: string, api_root: string, fetch_root: string, consumer: string, controlled: string, output: string] {
  checked $ledger_path $condition $profile | ignore
  let py = '
import hashlib,json,os,pathlib,re,stat,sys,zipfile
l=json.load(open(sys.argv[1])); condition,profile=sys.argv[2:4]; root=pathlib.Path(sys.argv[4]); fetch=pathlib.Path(sys.argv[5]); consumer=pathlib.Path(sys.argv[6]); controlled=pathlib.Path(sys.argv[7]); out=pathlib.Path(sys.argv[8]); c=next(x for x in l["cells"] if x["condition"]==condition and x["profile"]==profile)
def fail(code): raise SystemExit(code)
def sha(b): return hashlib.sha256(b).hexdigest()
def file_sha(p): return sha(pathlib.Path(p).read_bytes())
def api(prefix,run_id,head,attempt,artifact_id,name,digest,cap):
  run=json.loads((root/(prefix+"-run.json")).read_bytes()); artifact=json.loads((root/(prefix+"-artifact.json")).read_bytes())
  if not (run.get("id")==run_id and run.get("head_sha")==head and run.get("run_attempt")==attempt and run.get("status")=="completed" and run.get("conclusion")=="success" and run.get("repository",{}).get("id")==l["repository_id"] and run.get("head_repository",{}).get("id")==l["repository_id"]): fail(prefix.upper()+"_RUN_API")
  wr=artifact.get("workflow_run",{})
  if not (artifact.get("id")==artifact_id and artifact.get("name")==name and artifact.get("expired") is False and artifact.get("digest")=="sha256:"+digest and artifact.get("size_in_bytes",0)>0 and artifact.get("size_in_bytes")<=cap and wr.get("id")==run_id and wr.get("head_sha")==head and wr.get("repository_id")==l["repository_id"] and wr.get("head_repository_id")==l["repository_id"]): fail(prefix.upper()+"_ARTIFACT_API")
  return artifact
def verified_zip(prefix,digest,cap,expanded_cap,expected_size):
  p=root/(prefix+".zip")
  if not p.is_file() or p.is_symlink() or p.stat().st_size!=expected_size or p.stat().st_size>cap or file_sha(p)!=digest: fail(prefix.upper()+"_ZIP_BYTES")
  z=zipfile.ZipFile(p); infos=z.infolist(); names=[x.filename for x in infos]
  if len(names)!=len(set(names)) or any(not re.fullmatch(r"[A-Za-z0-9_.+/-]+",n) or n.startswith("/") or ".." in n.split("/") or "." in n.split("/") for n in names): fail(prefix.upper()+"_ZIP_PATH")
  if any(not stat.S_ISREG(x.external_attr>>16) or x.flag_bits&1 or x.file_size>expanded_cap for x in infos) or sum(x.file_size for x in infos)>expanded_cap: fail(prefix.upper()+"_ZIP_SCOPE")
  if z.testzip() is not None: fail(prefix.upper()+"_ZIP_CRC")
  return z
api("pr117",l["pr117_run"],l["pr117_head"],l["pr117_attempt"],l["pr117_artifact_id"],"duckdb-matching-browser-candidate",l["pr117_zip_sha256"],90000000)
pr136_api=api("pr136",l["pr136_run"],l["pr136_head"],l["pr136_attempt"],l["pr136_artifact_id"],"duckdb-combined-custody-eh",l["pr136_zip_sha256"],l["max_producer_zip_bytes"])
pr137_api=api("pr137",l["pr137_run"],l["pr137_head"],l["pr137_attempt"],c["artifact_id"],"duckdb-eh-pair-"+condition+"-"+profile,c["artifact_zip_sha256"],l["max_trial_zip_bytes"])
custody=json.loads((fetch/"custody.json").read_bytes())
if not (custody.get("custody_commit")==l["pr117_custody_commit"] and custody.get("producer_run")==l["pr117_run"] and custody.get("producer_head")==l["pr117_head"] and custody.get("artifact_id")==l["pr117_artifact_id"] and custody.get("original_artifact_digest")=="sha256:"+l["pr117_zip_sha256"] and custody.get("admission")=="NO"): fail("PR117_CUSTODY")
for name in ("duckdb-browser.mjs","duckdb-browser-mvp.worker.js","duckdb-mvp.wasm"):
  if file_sha(fetch/"matching-output"/name)!=l["assets"][name] or file_sha(consumer/"app/leptos-ui/duckdb-runtime"/name)!=l["assets"][name]: fail("PR117_ASSET_BYTES")
with verified_zip("pr136",l["pr136_zip_sha256"],l["max_producer_zip_bytes"],419430400,pr136_api["size_in_bytes"]) as z:
  bill=z.read("selected-native-glue-bill-v1.json"); wasm=z.read(condition+".wasm"); js=z.read(condition+".js")
  if sha(bill)!=l["pr136_bill_sha256"] or sha(wasm)!=l["assets"]["duckdb-eh.wasm"][condition]: fail("PR136_SELECTED_BYTES")
  producer_bill=json.loads(bill)
  if producer_bill.get("schema")!="selected-native-glue-bill/v1": fail("PR136_BILL_SCHEMA")
with verified_zip("pr137",c["artifact_zip_sha256"],l["max_trial_zip_bytes"],16777216,pr137_api["size_in_bytes"]) as z:
  pair_bytes=z.read("pair.json"); worker=z.read("rebuilt-worker.js"); selected=z.read("installed-selected-bill.json"); legal_bytes=z.read("installed-legal-custody.json")
  if sha(pair_bytes)!=c["pair_sha256"] or sha(worker)!=l["assets"]["duckdb-browser-eh.worker.js"] or sha(selected)!=c["bill_sha256"] or sha(legal_bytes)!=c["custody_sha256"]: fail("PR137_SELECTED_BYTES")
  pair=json.loads(pair_bytes); v5=json.loads(selected); legal=json.loads(legal_bytes)
  if not (pair.get("condition")==condition and pair.get("worker_sha256")==sha(worker) and pair.get("native_js_sha256")==sha(js) and v5.get("schema")=="tundra-duckdb-installed-json-package-selected-bill/v5" and v5.get("condition")==condition and v5.get("profile")==profile and v5.get("producer_run")==l["pr136_run"] and v5.get("producer_bill_sha256")==l["pr136_bill_sha256"] and v5.get("condition_output",{}).get("wasm_sha256")==sha(wasm) and v5.get("condition_output",{}).get("js_sha256")==sha(js) and legal.get("schema")=="tundra-eh-installed-notice-custody/v2" and legal.get("condition")==condition and legal.get("profile")==profile and legal.get("installed_bill_sha256")==sha(selected)): fail("PR137_PAIR_BILL_JOIN")
  assets={x["path"].split("/")[-1]:x["sha256"] for x in v5["assets"]}
  expected={k:(v[condition] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
  if assets!=expected or len(v5.get("texts",[]))!=37 or len(v5.get("selected_notice_joins",[]))!=71 or len(v5.get("selected_unresolved_keys",[]))!=35 or v5.get("legal_admission")!="NO": fail("PR137_SELECTED_GRAPH")
  notice_hashes={x["file"].split("/")[-1]:x["sha256"] for x in v5["texts"]}
  if len(notice_hashes)!=37 or len(legal.get("installed_texts",{}))!=37: fail("PR137_NOTICE_SET")
  target=consumer/"app/tauri/third-party-licenses/ui/duckdb-runtime"
  for name,digest in notice_hashes.items():
    member="installed-notice-bytes/"+name
    if member not in z.namelist() or sha(z.read(member))!=digest or legal["installed_texts"].get("third-party-licenses/ui/duckdb-runtime/"+name)!=digest: fail("PR137_NOTICE_BYTES")
    if name!="native-yyjson-0.6.0-LICENSE" and file_sha(target/name)!=digest: fail("PR119_NOTICE_JOIN")
  (root/"trial-selected-bill.json").write_bytes(selected)
  (root/"trial-notice-hashes.json").write_text(json.dumps(notice_hashes,sort_keys=True))
runtime=consumer/"app/leptos-ui/duckdb-runtime"
if file_sha(runtime/"duckdb-browser-eh.worker.js")!=l["assets"]["duckdb-browser-eh.worker.js"]: fail("ORIGINAL_WORKER_DRIFT")
if file_sha(runtime/"duckdb-eh.wasm")==l["assets"]["duckdb-eh.wasm"][condition]: fail("NEW_WASM_ALREADY_PRESENT")
(runtime/"duckdb-browser-eh.worker.js").write_bytes(worker)
(runtime/"duckdb-eh.wasm").write_bytes(wasm)
for name,digest in expected.items():
  if file_sha(runtime/name)!=digest: fail("STAGED_ASSET_DRIFT")
(root/"producer-selected-bill.json").write_bytes(bill)
(root/"condition.js").write_bytes(js)
archive=controlled/(condition+"-libnoexit.a"); notice=controlled/"emscripten-LICENSE"
if file_sha(archive)!=l["archive_sha256"] or file_sha(notice)!=l["notice_sha256"]: fail("CONTROLLED_RESOURCE_BYTES")
if (target/"native-emscripten-libnoexit.a").exists() or (target/"native-emscripten-libnoexit-cell.json").exists(): fail("CONTROLLED_RESOURCE_PREEXISTS")
(target/"native-emscripten-libnoexit.a").write_bytes(archive.read_bytes())
(target/"native-emscripten-LICENSE").write_bytes(notice.read_bytes())
cell={"schema":"tundra-libnoexit-new-runtime-resource-cell/v1","condition":condition,"profile":profile,"consumer_head":l["consumer_head"],"pr117_artifact_id":l["pr117_artifact_id"],"pr136_artifact_id":l["pr136_artifact_id"],"pr137_artifact_id":c["artifact_id"],"pr137_selected_bill_sha256":c["bill_sha256"],"assets":expected,"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"archive_linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","selected_graph_exhaustive":"NO","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
(target/"native-emscripten-libnoexit-cell.json").write_text(json.dumps(cell,sort_keys=True)+"\n")
print(json.dumps({"schema":l["schema"],"status":"THREE_PRODUCER_INPUTS_AND_FIVE_SOURCE_ASSETS_AUTHENTICATED_ONLY","condition":condition,"profile":profile,"pr117_artifact_id":l["pr117_artifact_id"],"pr136_artifact_id":l["pr136_artifact_id"],"pr137_artifact_id":c["artifact_id"],"selected_bill_sha256":c["bill_sha256"],"cell_resource_sha256":file_sha(target/"native-emscripten-libnoexit-cell.json"),"package_install_verified":"NO","behavior_cases":"NOT_RUN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $condition $profile $api_root $fetch_root $consumer $controlled $output])
  require ($receipt.status == 'THREE_PRODUCER_INPUTS_AND_FIVE_SOURCE_ASSETS_AUTHENTICATED_ONLY') 'STAGE_SCOPE'
  $receipt | to json --raw | save $output
}

def package [ledger_path: string, condition: string, profile: string, consumer: string, api_root: string, stage_receipt: string, source_receipt: string, preflight: string, deb: string, extracted: string, embedded: string, gate_head: string, run_id: string, run_attempt: string, candidate: string] {
  checked $ledger_path $condition $profile | ignore
  let py = '
import hashlib,json,os,pathlib,shutil,sys
l=json.load(open(sys.argv[1])); condition,profile=sys.argv[2:4]; consumer=pathlib.Path(sys.argv[4]); api=pathlib.Path(sys.argv[5]); stage=json.load(open(sys.argv[6])); source=json.load(open(sys.argv[7])); preflight=json.load(open(sys.argv[8])); deb=pathlib.Path(sys.argv[9]); extracted=pathlib.Path(sys.argv[10]); embedded=json.load(open(sys.argv[11])); head=sys.argv[12]; run_id=int(sys.argv[13]); attempt=int(sys.argv[14]); candidate=pathlib.Path(sys.argv[15]); c=next(x for x in l["cells"] if x["condition"]==condition and x["profile"]==profile)
def fail(code): raise SystemExit(code)
def sha(p): return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def regular(p): return pathlib.Path(p).is_file() and not pathlib.Path(p).is_symlink()
if not (len(head)==40 and all(x in "0123456789abcdef" for x in head) and run_id>0 and attempt>0 and not candidate.exists()): fail("PACKAGE_ARGUMENTS")
if not (stage.get("status")=="THREE_PRODUCER_INPUTS_AND_FIVE_SOURCE_ASSETS_AUTHENTICATED_ONLY" and stage.get("condition")==condition and stage.get("profile")==profile and stage.get("selected_bill_sha256")==c["bill_sha256"] and source.get("status")=="EXACT_PR137_SOURCE_TRANSFORMATION_ONLY" and source.get("final_main_sha256")==l["final_main_sha256"] and preflight.get("status")=="PRODUCER_BYTES_AUTHENTICATED_ONLY" and preflight.get("legal_admission")=="NO"): fail("SOURCE_RECEIPT_JOIN")
if sha(consumer/"app/tauri/src/main.rs")!=l["final_main_sha256"]: fail("BUILT_SOURCE_DRIFT")
bill=consumer/"app/tauri/third-party-licenses/ui/duckdb-runtime-selected-bill.json"
if not regular(bill) or sha(bill)!=c["bill_sha256"] or bill.read_bytes()!=(api/"trial-selected-bill.json").read_bytes(): fail("V5_BILL_BYTES")
v5=json.loads(bill.read_bytes())
if not (v5.get("schema")=="tundra-duckdb-installed-json-package-selected-bill/v5" and v5.get("condition")==condition and v5.get("profile")==profile and v5.get("legal_admission")=="NO" and v5.get("generator_input_closure")=="UNPROVEN" and len(v5.get("texts",[]))==37 and len(v5.get("selected_notice_joins",[]))==71 and len(v5.get("selected_unresolved_keys",[]))==35): fail("V5_BILL_SCOPE")
assets={k:(v[condition] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
asset_rows={x["key"]:x for x in embedded.get("assets",[]) if x.get("key","").startswith("duckdb-runtime/")}
if set(asset_rows)!={"duckdb-runtime/"+n for n in assets}: fail("EMBEDDED_ASSET_SET")
for name,digest in assets.items():
  src=consumer/"app/leptos-ui/duckdb-runtime"/name; dist=consumer/"app/leptos-ui/dist/duckdb-runtime"/name; row=asset_rows["duckdb-runtime/"+name]
  if not (regular(src) and regular(dist) and sha(src)==sha(dist)==digest and row.get("dist_sha256")==digest and row.get("embedded_sha256")==digest and row.get("transform")=="identity"): fail("SOURCE_DIST_EMBEDDED_BYTES")
if not regular(deb) or deb.stat().st_size>l["max_package_bytes"] or deb.stat().st_size<=0 or sha(deb) in l["retired_pr151_deb_sha256"]: fail("PACKAGE_BYTES")
elf=extracted/"usr/bin/auto-tundra"; resource=extracted/"usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime"; selected=extracted/"usr/lib/auto-tundra/third-party-licenses/ui/duckdb-runtime-selected-bill.json"
if not regular(elf) or not elf.read_bytes().startswith(b"\x7fELF\x02\x01\x01") or not regular(selected) or sha(selected)!=c["bill_sha256"]: fail("EXTRACTED_ELF_OR_BILL")
notice_hashes=json.loads((api/"trial-notice-hashes.json").read_bytes())
if len(notice_hashes)!=37: fail("NOTICE_SET")
source_notices=consumer/"app/tauri/third-party-licenses/ui/duckdb-runtime"
for name,digest in notice_hashes.items():
  if not (regular(source_notices/name) and regular(resource/name) and sha(source_notices/name)==sha(resource/name)==digest): fail("SOURCE_PACKAGE_NOTICE_JOIN")
for name,digest in (("native-emscripten-LICENSE",l["notice_sha256"]),("native-emscripten-libnoexit.a",l["archive_sha256"])):
  if not regular(resource/name) or sha(resource/name)!=digest: fail("CONTROLLED_PACKAGE_RESOURCE")
cell_file=resource/"native-emscripten-libnoexit-cell.json"
if not regular(cell_file) or sha(cell_file)!=stage.get("cell_resource_sha256"): fail("CONTROLLED_CELL_BYTES")
cell=json.loads(cell_file.read_bytes())
if not (cell.get("condition")==condition and cell.get("profile")==profile and cell.get("pr137_selected_bill_sha256")==c["bill_sha256"] and cell.get("archive_linker_input")=="UNPROVEN" and cell.get("legal_admission")=="NO"): fail("CONTROLLED_CELL_SCOPE")
candidate.mkdir(); witness=candidate/"witness"; witness.mkdir()
shutil.copyfile(deb,candidate/"package.deb");shutil.copyfile(bill,candidate/"installed-selected-bill.json");shutil.copyfile(consumer/"gate-source/.github/receipts/duckdb-libnoexit-corpus-package-source-v1.json",candidate/"corpus-source-ledger.json");shutil.copyfile(pathlib.Path(sys.argv[7]),candidate/"source-receipt.json");shutil.copyfile(pathlib.Path(sys.argv[8]),candidate/"preflight.json")
shutil.copyfile(resource/"native-emscripten-libnoexit.a",witness/"installed-archive");shutil.copyfile(resource/"native-emscripten-LICENSE",witness/"installed-notice");shutil.copyfile(cell_file,witness/"installed-cell.json")
identity={"schema":"tundra-libnoexit-new-runtime-package-identity/v1","status":"NEW_RUNTIME_PACKAGE_BYTES_DIAGNOSTIC_ONLY","condition":condition,"profile":profile,"gate_head":head,"run_id":run_id,"run_attempt":attempt,"consumer_head":l["consumer_head"],"pr117_artifact_id":l["pr117_artifact_id"],"pr136_artifact_id":l["pr136_artifact_id"],"pr137_artifact_id":c["artifact_id"],"pr137_zip_sha256":c["artifact_zip_sha256"],"source_ledger_sha256":sha(sys.argv[1]),"corpus_source_ledger_sha256":l["corpus_source_ledger_sha256"],"source_receipt_sha256":sha(pathlib.Path(sys.argv[7])),"preflight_sha256":sha(pathlib.Path(sys.argv[8])),"corpus_source_sha256":l["final_main_sha256"],"selected_bill_sha256":c["bill_sha256"],"package_sha256":sha(deb),"package_bytes":deb.stat().st_size,"extracted_elf_sha256":sha(elf),"cell_resource_sha256":sha(cell_file),"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"assets":assets,"notice_text_count":37,"selected_notice_joins":71,"unresolved_selected_keys":35,"package_install_verified":"NO","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","selected_graph_exhaustive":"NO","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
(candidate/"corpus-package-identity.json").write_text(json.dumps(identity,sort_keys=True)+"\n")
(witness/"cell-witness.json").write_text(json.dumps({"schema":"tundra-libnoexit-new-runtime-package-witness/v1","condition":condition,"profile":profile,"package_sha256":identity["package_sha256"],"installed_elf_sha256":identity["extracted_elf_sha256"],"selected_bill_sha256":c["bill_sha256"],"cell_resource_sha256":identity["cell_resource_sha256"],"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"source_dist_embedded_assets":"FIVE_IDENTICAL_HASHES","package_install_verified":"NO","behavior_cases":"NOT_RUN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"},sort_keys=True)+"\n")
print(json.dumps(identity,sort_keys=True))
'
  let receipt = (run_python $py [$ledger_path $condition $profile $consumer $api_root $stage_receipt $source_receipt $preflight $deb $extracted $embedded $gate_head $run_id $run_attempt $candidate])
  require ($receipt.status == 'NEW_RUNTIME_PACKAGE_BYTES_DIAGNOSTIC_ONLY' and $receipt.legal_admission == 'NO') 'PACKAGE_SCOPE'
}

def seal [ledger_path: string, candidate: string, selected: string] {
  let py = '
import hashlib,json,pathlib,re,shutil,sys
l=json.load(open(sys.argv[1])); source=pathlib.Path(sys.argv[2]); target=pathlib.Path(sys.argv[3])
def fail(code): raise SystemExit(code)
names=["package.deb","preflight.json","installed-selected-bill.json","corpus-source-ledger.json","source-receipt.json","corpus-package-identity.json","cleanup-proof.json","witness/cell-witness.json","witness/installed-archive","witness/installed-cell.json","witness/installed-notice"]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def inspect(root):
  if not root.is_dir() or root.is_symlink(): fail("ARTIFACT_ROOT")
  if sorted(p.name for p in root.iterdir())!=sorted([n for n in names if "/" not in n]+["witness"]): fail("ARTIFACT_TOP_SET")
  witness=root/"witness"
  if not witness.is_dir() or witness.is_symlink() or sorted(p.name for p in witness.iterdir())!=sorted(n.split("/",1)[1] for n in names if n.startswith("witness/")): fail("ARTIFACT_WITNESS_SET")
  if any(not (root/n).is_file() or (root/n).is_symlink() for n in names): fail("ARTIFACT_MEMBER_SET")
  sizes={n:(root/n).stat().st_size for n in names}
  if any(v<=0 or v>(l["max_package_bytes"] if n=="package.deb" else 131072) for n,v in sizes.items()) or sum(sizes.values())>l["max_package_bytes"]: fail("ARTIFACT_SIZE")
  i=json.loads((root/"corpus-package-identity.json").read_bytes()); w=json.loads((root/"witness/cell-witness.json").read_bytes()); cleanup=json.loads((root/"cleanup-proof.json").read_bytes())
  c=next((x for x in l["cells"] if x["condition"]==i.get("condition") and x["profile"]==i.get("profile")),None)
  if c is None or re.fullmatch(r"[0-9a-f]{40}",str(i.get("gate_head",""))) is None or not isinstance(i.get("run_id"),int) or i["run_id"]<=0 or not isinstance(i.get("run_attempt"),int) or i["run_attempt"]<=0 or re.fullmatch(r"[0-9a-f]{64}",str(i.get("extracted_elf_sha256",""))) is None: fail("ARTIFACT_RUN_SCOPE")
  assets={k:(v[i["condition"]] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
  expected={"schema":"tundra-libnoexit-new-runtime-package-identity/v1","status":"NEW_RUNTIME_PACKAGE_BYTES_DIAGNOSTIC_ONLY","condition":c["condition"],"profile":c["profile"],"gate_head":i["gate_head"],"run_id":i["run_id"],"run_attempt":i["run_attempt"],"consumer_head":l["consumer_head"],"pr117_artifact_id":l["pr117_artifact_id"],"pr136_artifact_id":l["pr136_artifact_id"],"pr137_artifact_id":c["artifact_id"],"pr137_zip_sha256":c["artifact_zip_sha256"],"source_ledger_sha256":sha(pathlib.Path(sys.argv[1])),"corpus_source_ledger_sha256":l["corpus_source_ledger_sha256"],"source_receipt_sha256":sha(root/"source-receipt.json"),"preflight_sha256":sha(root/"preflight.json"),"corpus_source_sha256":l["final_main_sha256"],"selected_bill_sha256":c["bill_sha256"],"package_sha256":sha(root/"package.deb"),"package_bytes":sizes["package.deb"],"extracted_elf_sha256":i["extracted_elf_sha256"],"cell_resource_sha256":sha(root/"witness/installed-cell.json"),"archive_sha256":sha(root/"witness/installed-archive"),"notice_sha256":sha(root/"witness/installed-notice"),"assets":assets,"notice_text_count":37,"selected_notice_joins":71,"unresolved_selected_keys":35,"package_install_verified":"NO","behavior_cases":"NOT_RUN","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","selected_graph_exhaustive":"NO","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
  if i!=expected or i["archive_sha256"]!=l["archive_sha256"] or i["notice_sha256"]!=l["notice_sha256"] or i["selected_bill_sha256"]!=sha(root/"installed-selected-bill.json") or sha(root/"corpus-source-ledger.json")!=l["corpus_source_ledger_sha256"]: fail("ARTIFACT_IDENTITY")
  expected_witness={"schema":"tundra-libnoexit-new-runtime-package-witness/v1","condition":c["condition"],"profile":c["profile"],"package_sha256":i["package_sha256"],"installed_elf_sha256":i["extracted_elf_sha256"],"selected_bill_sha256":c["bill_sha256"],"cell_resource_sha256":i["cell_resource_sha256"],"archive_sha256":l["archive_sha256"],"notice_sha256":l["notice_sha256"],"source_dist_embedded_assets":"FIVE_IDENTICAL_HASHES","package_install_verified":"NO","behavior_cases":"NOT_RUN","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
  if w!=expected_witness: fail("ARTIFACT_WITNESS")
  source=json.loads((root/"source-receipt.json").read_bytes()); preflight=json.loads((root/"preflight.json").read_bytes()); bill=json.loads((root/"installed-selected-bill.json").read_bytes()); cell=json.loads((root/"witness/installed-cell.json").read_bytes())
  if source.get("schema")!="tundra-libnoexit-corpus-package-source/v1" or source.get("status")!="EXACT_PR137_SOURCE_TRANSFORMATION_ONLY" or source.get("final_main_sha256")!=l["final_main_sha256"] or preflight.get("status")!="PRODUCER_BYTES_AUTHENTICATED_ONLY" or preflight.get("legal_admission")!="NO" or bill.get("schema")!="tundra-duckdb-installed-json-package-selected-bill/v5" or bill.get("condition")!=c["condition"] or bill.get("profile")!=c["profile"] or bill.get("legal_admission")!="NO" or cell.get("schema")!="tundra-libnoexit-new-runtime-resource-cell/v1" or cell.get("condition")!=c["condition"] or cell.get("profile")!=c["profile"] or cell.get("assets")!=assets or cell.get("pr137_artifact_id")!=c["artifact_id"] or cell.get("pr137_selected_bill_sha256")!=c["bill_sha256"] or cell.get("legal_admission")!="NO": fail("ARTIFACT_PROVENANCE")
  if cleanup!={"schema":"tundra-libnoexit-new-runtime-package-cleanup/v1","status":"BUILD_AND_DOWNLOAD_INPUTS_REMOVED","ephemeral_cleanup":"PROVEN","package_install_verified":"NO","behavior_cases":"NOT_RUN","product_admission":"NO","fleet_admission":"NO"}: fail("ARTIFACT_CLEANUP")
inspect(source)
if target.exists() or target.is_symlink(): fail("SELECTED_PREEXISTS")
target.mkdir();(target/"witness").mkdir()
for n in names: shutil.copyfile(source/n,target/n)
inspect(target)
print(json.dumps({"status":"EXACT_ELEVEN_MEMBERS_SEALED","legal_admission":"NO"}))
'
  let receipt = (run_python $py [$ledger_path $candidate $selected])
  require ($receipt.status == 'EXACT_ELEVEN_MEMBERS_SEALED') 'SEAL_SCOPE'
}

def main [mode: string, ...args: string] {
  match $mode {
    'stage' => { require (($args | length) == 8) 'ARGS'; stage $args.0 $args.1 $args.2 $args.3 $args.4 $args.5 $args.6 $args.7 }
    'package' => { require (($args | length) == 15) 'ARGS'; package $args.0 $args.1 $args.2 $args.3 $args.4 $args.5 $args.6 $args.7 $args.8 $args.9 $args.10 $args.11 $args.12 $args.13 $args.14 }
    'seal' => { require (($args | length) == 3) 'ARGS'; seal $args.0 $args.1 $args.2 }
    _ => { error make {msg: 'MODE'} }
  }
}
