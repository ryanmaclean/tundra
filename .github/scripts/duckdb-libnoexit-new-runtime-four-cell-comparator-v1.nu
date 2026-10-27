#!/usr/bin/env nu

# Exact PR154 phase-bearing disposable receipts; diagnostic aggregate only.
def require [ok: bool, code: string] { if not $ok { error make {msg: $code} } }

def main [ledger_path: string, input_root: string, output: string] {
  let l = (open $ledger_path)
  require ($l.schema == 'tundra-libnoexit-new-runtime-four-cell-comparator/v1' and $l.scope == 'EXACT_PR154_PHASE_BEARING_FOUR_CELL_DIAGNOSTIC_ONLY') 'LEDGER_SCOPE'
  require ($l.repository == 'ryanmaclean/tundra' and $l.repository_id == 1164936674 and $l.source_head == 'f5bc996490857ac7c724e72f86f5aee69c0b5320' and $l.run_id == 40596646042 and $l.run_attempt == 1 and $l.producer_head == '1cf16aa6c5c0fa6265e06ccc03ad9ef5e3f35875' and $l.producer_run_id == 40588647258) 'LEDGER_RUN'
  require ($l.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and $l.harness_sha256 == '57480e689cccfa26e8527dba3f566c275bfbfad7cb2c43c4cb3dd85263284b8e' and $l.semantic_sha256 == '008589b5e6b4bf27adecb46b47b2945b7cdc015793cedc3e86c3939c6571ac79') 'LEDGER_CORPUS'
  require ($l.archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86' and $l.max_artifact_zip_bytes == 65536 and $l.max_receipt_bytes == 131072) 'LEDGER_BYTES'
  require (($l.cells | each {|c| $"($c.condition)/($c.profile)"}) == ['image/default' 'image/sounds' 'clean/default' 'clean/sounds']) 'LEDGER_CELLS'
  require ($l.package_install_verified == 'FOUR_DISPOSABLE_RUNNERS_ONLY' and $l.behavior_cases == '32_PASSED_FOUR_DISPOSABLE_RUNNERS_ONLY' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'LEDGER_ADMISSION'
  let py = '
import hashlib,json,pathlib,re,stat,sys,zipfile
l=json.load(open(sys.argv[1])); root=pathlib.Path(sys.argv[2]); run=json.loads((root/"run.json").read_bytes())
def fail(code): raise SystemExit(code)
def sha(b): return hashlib.sha256(b).hexdigest()
if not (run.get("id")==l["run_id"] and run.get("run_attempt")==l["run_attempt"] and run.get("head_sha")==l["source_head"] and run.get("event")=="workflow_dispatch" and run.get("status")=="completed" and run.get("conclusion")=="success" and run.get("repository",{}).get("id")==l["repository_id"] and run.get("head_repository",{}).get("id")==l["repository_id"]): fail("RUN_IDENTITY")
ids="bridge-create bridge-json-insert bridge-empty-insert bridge-persist-null-utf8 bridge-macro-create bridge-macro-query bridge-parser-error bridge-parser-recovery bridge-catalog-error bridge-catalog-recovery bridge-binder-error bridge-conversion-error bridge-after-errors bridge-arrow-json-types bridge-large-bigint-string installed-local-pair-urls installed-local-pair-bytes direct-arrow-schema direct-csv-register direct-csv-scan direct-csv-insert direct-json-register direct-json-scan direct-json-insert direct-parquet-copy direct-parquet-buffer direct-parquet-reregister direct-parquet-scan direct-file-error direct-after-error direct-arrow-invalid-ipc direct-after-arrow-error".split()
phase_only={"installed-local-pair-urls","installed-local-pair-bytes","direct-parquet-buffer","direct-parquet-reregister"}
phase={x:("bridge" if x.startswith("bridge-") else "provenance" if x in ("installed-local-pair-urls","installed-local-pair-bytes") else "direct") for x in ids}
outer={"consumer_head","corpus_cases","corpus_passed","corpus_schema","csp_denied","csp_violations","diagnostic_install","harness_sha256","local_runtime","remote_requests","schema","semantic_paths","semantic_sha256","smoke_schema","source_head","status","unsupported_cases","process_group_quiescent","ephemeral_cleanup"}
install_fields={"assets","behavior_cases","condition","fleet_admission","generator_input_closure","installed_archive_sha256","installed_bill_sha256","installed_cell_sha256","installed_elf_sha256","installed_notice_count","installed_notice_sha256","legal_admission","linker_input","package_install_verified","package_sha256","producer_artifact_id","producer_run_id","product_admission","profile"}
summary=[]; snapshots=[]
for c in l["cells"]:
  key=c["condition"]+"-"+c["profile"]; d=root/key; zpath=d/"artifact.zip"
  if not zpath.is_file() or zpath.is_symlink() or zpath.stat().st_size>l["max_artifact_zip_bytes"] or zpath.stat().st_size<=0: fail("ZIP_SCOPE")
  meta=json.loads((d/"artifact.json").read_bytes()); wr=meta.get("workflow_run",{}); zbytes=zpath.read_bytes()
  expected_name="duckdb-libnoexit-new-runtime-installed-{}-{}-{}".format(l["run_id"],l["run_attempt"],key)
  if not (meta.get("id")==c["artifact_id"] and meta.get("name")==expected_name and meta.get("expired") is False and meta.get("size_in_bytes")==len(zbytes) and meta.get("digest")=="sha256:"+c["artifact_zip_sha256"] and wr.get("id")==l["run_id"] and wr.get("head_sha")==l["source_head"] and wr.get("repository_id")==l["repository_id"] and wr.get("head_repository_id")==l["repository_id"]): fail("ARTIFACT_API")
  if sha(zbytes)!=c["artifact_zip_sha256"]: fail("ZIP_HASH")
  with zipfile.ZipFile(zpath) as z:
    infos=z.infolist()
    if len(infos)!=1 or infos[0].filename!="receipt-final.json" or not stat.S_ISREG(infos[0].external_attr>>16) or infos[0].flag_bits&1 or infos[0].file_size>l["max_receipt_bytes"] or z.testzip() is not None: fail("ZIP_MEMBER_SCOPE")
    data=z.read("receipt-final.json")
  if sha(data)!=c["receipt_sha256"]: fail("RECEIPT_HASH")
  r=json.loads(data); di=r.get("diagnostic_install",{})
  if not isinstance(r,dict) or set(r)!=outer or not isinstance(di,dict) or set(di)!=install_fields: fail("RECEIPT_FIELDS")
  if not (r["schema"]=="tundra-libnoexit-new-runtime-installed-behavior/v1" and r["status"]=="INSTALLED_32_CASE_CORPUS_DIAGNOSTIC_ONLY" and r["smoke_schema"]=="tundra-duckdb-webkit-smoke/v1" and r["source_head"]==l["producer_head"] and r["consumer_head"]==l["consumer_head"] and r["harness_sha256"]==l["harness_sha256"] and r["semantic_sha256"]==l["semantic_sha256"] and r["semantic_paths"]==9 and r["corpus_schema"]=="tundra-installed-eh-corpus/v1" and r["corpus_passed"] is True and r["unsupported_cases"]==["arrow-positive-ipc","multibatch-stream","upstream-full-suite"] and r["remote_requests"]==0 and r["local_runtime"] is True and r["csp_denied"] is True and r["csp_violations"]==0 and r["process_group_quiescent"]=="PROVEN" and r["ephemeral_cleanup"]=="PROVEN"): fail("RECEIPT_SCOPE")
  assets={k:(v[c["condition"]] if isinstance(v,dict) else v) for k,v in l["assets"].items()}
  expected_install={"assets":assets,"behavior_cases":"32_PASSED_DISPOSABLE_RUNNER_ONLY","condition":c["condition"],"fleet_admission":"NO","generator_input_closure":"UNPROVEN","installed_archive_sha256":l["archive_sha256"],"installed_bill_sha256":c["bill_sha256"],"installed_cell_sha256":c["cell_resource_sha256"],"installed_elf_sha256":c["elf_sha256"],"installed_notice_count":37,"installed_notice_sha256":l["notice_sha256"],"legal_admission":"NO","linker_input":"UNPROVEN","package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","package_sha256":c["package_sha256"],"producer_artifact_id":c["producer_artifact_id"],"producer_run_id":l["producer_run_id"],"product_admission":"NO","profile":c["profile"]}
  if di!=expected_install: fail("PACKAGE_IDENTITY")
  cases=r["corpus_cases"]
  if not isinstance(cases,list) or len(cases)!=32 or any(not isinstance(x,dict) or set(x)!={"id","phase","passed","case_sha256"} for x in cases): fail("CASE_SHAPE")
  observed=[x["id"] for x in cases]
  if len(set(observed))!=32 or sorted(observed)!=sorted(ids): fail("CASE_SET")
  if any(x["phase"]!=phase[x["id"]] or x["passed"] is not True or re.fullmatch(r"[0-9a-f]{64}",x["case_sha256"]) is None for x in cases): fail("CASE_SCOPE")
  snapshots.append({x["id"]:x for x in cases})
  summary.append({"condition":c["condition"],"profile":c["profile"],"artifact_id":c["artifact_id"],"artifact_zip_sha256":c["artifact_zip_sha256"],"receipt_sha256":c["receipt_sha256"],"package_sha256":c["package_sha256"],"elf_sha256":c["elf_sha256"],"bill_sha256":c["bill_sha256"],"cell_resource_sha256":c["cell_resource_sha256"],"assets":assets,"installed_notice_count":37,"package_install_verified":"YES_DISPOSABLE_RUNNER_ONLY","behavior_cases":"32_PASSED_DISPOSABLE_RUNNER_ONLY"})
semantic=[]; phase_comparison=[]
for id in ids:
  rows=[s[id] for s in snapshots]
  if id in phase_only:
    if len({(x["phase"],x["passed"]) for x in rows})!=1: fail("PHASE_ONLY_MISMATCH")
    phase_comparison.append({"id":id,"phase":rows[0]["phase"],"passed":True})
  else:
    if len({x["case_sha256"] for x in rows})!=1 or len({(x["phase"],x["passed"]) for x in rows})!=1: fail("SEMANTIC_CASE_MISMATCH")
    semantic.append({"id":id,"phase":rows[0]["phase"],"passed":True,"case_sha256":rows[0]["case_sha256"]})
if len(semantic)!=28 or len(phase_comparison)!=4: fail("COMPARISON_COUNT")
out={"schema":l["schema"],"status":"FOUR_CELL_INSTALLED_CORPUS_COMPARISON_DIAGNOSTIC_ONLY","source_head":l["source_head"],"run_id":l["run_id"],"run_attempt":l["run_attempt"],"producer_head":l["producer_head"],"producer_run_id":l["producer_run_id"],"consumer_head":l["consumer_head"],"harness_sha256":l["harness_sha256"],"semantic_sha256":l["semantic_sha256"],"semantic_case_signatures":semantic,"phase_only_cases":phase_comparison,"cells":summary,"package_install_verified":"FOUR_DISPOSABLE_RUNNERS_ONLY","behavior_cases":"32_PASSED_FOUR_DISPOSABLE_RUNNERS_ONLY","linker_input":"UNPROVEN","generator_input_closure":"UNPROVEN","selected_graph_exhaustive":"NO","legal_admission":"NO","product_admission":"NO","fleet_admission":"NO"}
print(json.dumps(out,sort_keys=True))
'
  let result = (^python3 -c $py $ledger_path $input_root | complete)
  require ($result.exit_code == 0) ($result.stderr | str trim | default 'COMPARISON_REFUSED')
  let receipt = ($result.stdout | from json)
  require ($receipt.status == 'FOUR_CELL_INSTALLED_CORPUS_COMPARISON_DIAGNOSTIC_ONLY' and ($receipt.semantic_case_signatures | length) == 28 and ($receipt.phase_only_cases | length) == 4 and $receipt.legal_admission == 'NO') 'COMPARISON_SCOPE'
  require (($receipt | to json --raw | str length) <= 131072) 'RECEIPT_CAP'
  $receipt | to json --raw | save $output
}
