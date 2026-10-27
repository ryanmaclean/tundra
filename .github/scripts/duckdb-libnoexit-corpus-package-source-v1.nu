#!/usr/bin/env nu

# Exact PR137 diagnostic source transformation and new PR148-style package identity.
def require [ok: bool, reason: string] { if not $ok { error make {msg: $reason} } }
def sha [path: string] {
  let r = (^sha256sum -- $path | complete)
  require ($r.exit_code == 0) 'HASH_FAILED'
  $r.stdout | split row ' ' | first
}
def regular [path: string] {
  let rows = (try { ls -lD $path } catch { [] })
  require (($rows | length) == 1 and ($rows | first).type == 'file') 'NOT_REGULAR'
  ($rows | first).size | into int
}
def ledger [path: string] {
  let l = (open $path)
  require ($l.schema == 'tundra-libnoexit-corpus-package-source/v1' and $l.scope == 'PR119_FOUR_NEW_DIAGNOSTIC_CORPUS_PACKAGES_ONLY') 'LEDGER_SCOPE'
  require ($l.consumer_head == 'ad0df7182cd4c9ea4a83bac04e5901de0218d9d0' and $l.pr137_source_head == 'a5b05fc1c6e51add369fd61ffe668d7a4a293bc8') 'SOURCE_HEAD'
  require ($l.preimage_main_sha256 == 'e4a89f0db2603d1a6cd5d22b564cc03816b379116deb33c0322ea1d813bdae1d' and $l.semantic_main_sha256 == '667a6112b3256a2d6478e128808469a26a8dd0b566fac4f3a7d6ea32f905350d' and $l.final_main_sha256 == '57480e689cccfa26e8527dba3f566c275bfbfad7cb2c43c4cb3dd85263284b8e') 'SOURCE_HASHES'
  require ($l.semantic_patch_sha256 == '28deafac7a2f76e83adfbc92d4368444adbf97fae3aba496080f2f5a39742cca' and ($l.semantic_patch | str length) <= 8192) 'PATCH_PIN'
  require ($l.corpus_module_sha256 == '196ba238054a5b22e2a89d781eb7ed3844a416b71384f98fbc9c032eea551c0a' and $l.corpus_apply_helper_sha256 == '7895597e2417426c141dcbc7c5e002269fb5f7f3c3381a6612bdfba33af3acf4') 'CORPUS_PIN'
  require ($l.controlled_archive_sha256 == 'fdaa14b94400917928bc73415535f02244549987beb92719882bcc905b4280fe' and $l.controlled_notice_sha256 == '620a78084fc7ca97c0b5dea9abf891f3ffcadfdbf305276f099c9c4e12fc1d86') 'RESOURCE_PIN'
  require ($l.retired_pr148_packages_sha256 == ['7f4e468c35258fdec2f76704dcb3080917b77d8002b7616e70a58ac9688d25a8' 'f8c9e9468c2cf394edcd473ec319b896d9d821ae4757ddec1395406e887c9e26' '3e853d30c5c53b70100ae9447880fe611ed7dd37b548bb5829f4f0d968c8d332' 'bdf9db89cb30721b1f4e94f03c900392198da017af1e0264c9e76546e3379679']) 'RETIRED_PACKAGE_PIN'
  require ($l.package_install_verified == 'NO' and $l.behavior_cases == 'NOT_RUN' and $l.linker_input == 'UNPROVEN' and $l.generator_input_closure == 'UNPROVEN' and $l.selected_graph_exhaustive == 'NO' and $l.legal_admission == 'NO' and $l.product_admission == 'NO' and $l.fleet_admission == 'NO') 'ADMISSION_PIN'
  $l
}

export def patch [ledger_path: string, main_rs: string, corpus_js: string, apply_helper: string, scratch: string, output: string] {
  let l = (ledger $ledger_path)
  require ($main_rs == 'app/tauri/src/main.rs') 'MAIN_PATH'
  require ((regular $main_rs) > 0 and (sha $main_rs) == $l.preimage_main_sha256) 'PREIMAGE_DRIFT'
  require ((regular $corpus_js) > 0 and (sha $corpus_js) == $l.corpus_module_sha256) 'CORPUS_MODULE_DRIFT'
  require ((regular $apply_helper) > 0 and (sha $apply_helper) == $l.corpus_apply_helper_sha256) 'CORPUS_HELPER_DRIFT'
  ^mkdir -p -- $scratch
  let patch_file = ($scratch | path join 'semantic.patch')
  $l.semantic_patch | save --force $patch_file
  require ((regular $patch_file) <= 8192 and (sha $patch_file) == $l.semantic_patch_sha256) 'PATCH_BYTES_DRIFT'
  let check = (^git apply --check $patch_file | complete)
  require ($check.exit_code == 0) 'PATCH_PREIMAGE_REFUSED'
  let apply = (^git apply $patch_file | complete)
  require ($apply.exit_code == 0 and (sha $main_rs) == $l.semantic_main_sha256) 'SEMANTIC_PATCH_DRIFT'
  let corpus = (^nu $apply_helper $main_rs $corpus_js | complete)
  require ($corpus.exit_code == 0) 'CORPUS_PATCH_REFUSED'
  let text = (open --raw $main_rs)
  let old = 'result.ok = result.ok && corpus.passed;'
  require (($text | split row $old | length) == 2) 'CORPUS_FLAG_ANCHOR'
  $text | str replace $old 'result.corpusPassed = corpus.passed;' | save --force $main_rs
  require ((sha $main_rs) == $l.final_main_sha256) 'FINAL_SOURCE_DRIFT'
  let node = (^node --check $corpus_js | complete)
  require ($node.exit_code == 0) 'CORPUS_PARSE_REFUSED'
  let receipt = {schema: 'tundra-libnoexit-corpus-package-source/v1', status: 'EXACT_PR137_SOURCE_TRANSFORMATION_ONLY', consumer_head: $l.consumer_head, pr137_source_head: $l.pr137_source_head, preimage_main_sha256: $l.preimage_main_sha256, semantic_patch_sha256: $l.semantic_patch_sha256, semantic_main_sha256: $l.semantic_main_sha256, corpus_module_sha256: $l.corpus_module_sha256, corpus_apply_helper_sha256: $l.corpus_apply_helper_sha256, final_main_sha256: $l.final_main_sha256, package_install_verified: 'NO', behavior_cases: 'NOT_RUN', linker_input: 'UNPROVEN', generator_input_closure: 'UNPROVEN', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'}
  $receipt | to json --raw | save $output
  require ((regular $output) <= 8192) 'SOURCE_RECEIPT_CAP'
}

export def package [ledger_path: string, source_receipt: string, old_witness: string, deb: string, extracted_elf: string, condition: string, profile: string, gate_head: string, run_id: int, run_attempt: int, output: string] {
  let l = (ledger $ledger_path)
  require ($condition in ['image' 'clean'] and $profile in ['default' 'sounds'] and $gate_head =~ '^[0-9a-f]{40}$' and $run_id > 0 and $run_attempt > 0) 'CELL_ARGUMENTS'
  require ((sha 'app/tauri/src/main.rs') == $l.final_main_sha256) 'BUILT_SOURCE_DRIFT'
  require ((regular $source_receipt) <= 8192 and (regular $old_witness) <= 2097152) 'INPUT_RECEIPT_CAP'
  let source = (open $source_receipt)
  require ($source.schema == $l.schema and $source.status == 'EXACT_PR137_SOURCE_TRANSFORMATION_ONLY' and $source.final_main_sha256 == $l.final_main_sha256 and $source.semantic_patch_sha256 == $l.semantic_patch_sha256 and $source.behavior_cases == 'NOT_RUN') 'SOURCE_RECEIPT_DRIFT'
  let witness = (open $old_witness)
  require ($witness.schema == 'tundra-sdk-libnoexit-installed-cell-witness/v1' and $witness.status == 'PACKAGE_EXTRACTED_RESOURCE_BYTES_MATCH_DIAGNOSTIC_ONLY' and $witness.condition == $condition and $witness.profile == $profile) 'RESOURCE_WITNESS_DRIFT'
  require ($witness.archive_sha256 == $l.controlled_archive_sha256 and $witness.notice_sha256 == $l.controlled_notice_sha256 and $witness.package_install_verified == 'NO' and $witness.linker_input == 'UNPROVEN' and $witness.legal_admission == 'NO' and $witness.product_admission == 'NO' and $witness.fleet_admission == 'NO') 'RESOURCE_SCOPE_DRIFT'
  let deb_bytes = (regular $deb)
  require ($deb_bytes > 0 and $deb_bytes <= 201326592 and (sha $deb) == $witness.package_sha256) 'PACKAGE_BYTES_DRIFT'
  require ($witness.package_sha256 not-in $l.retired_pr148_packages_sha256) 'RETIRED_PACKAGE_SUBSTITUTION'
  require ((regular $extracted_elf) > 0) 'ELF_NOT_REGULAR'
  let receipt = {schema: 'tundra-libnoexit-corpus-package-identity/v1', status: 'NEW_CORPUS_PACKAGE_BYTES_DIAGNOSTIC_ONLY', condition: $condition, profile: $profile, gate_head: $gate_head, run_id: $run_id, run_attempt: $run_attempt, consumer_head: $l.consumer_head, corpus_source_sha256: $l.final_main_sha256, semantic_patch_sha256: $l.semantic_patch_sha256, package_sha256: (sha $deb), package_bytes: $deb_bytes, extracted_elf_sha256: (sha $extracted_elf), resource_witness_sha256: (sha $old_witness), cell_resource_sha256: $witness.cell_resource_sha256, archive_sha256: $witness.archive_sha256, notice_sha256: $witness.notice_sha256, package_install_verified: 'NO', behavior_cases: 'NOT_RUN', linker_input: 'UNPROVEN', generator_input_closure: 'UNPROVEN', selected_graph_exhaustive: 'NO', legal_admission: 'NO', product_admission: 'NO', fleet_admission: 'NO'}
  $receipt | to json --raw | save $output
  require ((regular $output) <= 8192) 'PACKAGE_RECEIPT_CAP'
}

def main [mode: string, ...args: string] {
  match $mode {
    'patch' => {
      require (($args | length) == 6) 'PATCH_ARGUMENTS'
      patch $args.0 $args.1 $args.2 $args.3 $args.4 $args.5
    }
    'package' => {
      require (($args | length) == 11) 'PACKAGE_ARGUMENTS'
      package $args.0 $args.1 $args.2 $args.3 $args.4 $args.5 $args.6 $args.7 ($args.8 | into int) ($args.9 | into int) $args.10
    }
    _ => { error make {msg: 'UNKNOWN_MODE'} }
  }
}
