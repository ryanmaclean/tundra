#!/usr/bin/env nu
# Apply only to a disposable remote workflow checkout after PR120's existing
# nine-path extension. The caller must verify the source/output hashes.

def replace_once [source: string, needle: string, replacement: string] {
  let parts = ($source | split row $needle)
  if (($parts | length) != 2) {
    error make {msg: $"expected one occurrence: ($needle)"}
  }
  $"($parts.0)($replacement)($parts.1)"
}

def main [main_rs: path, corpus_js: path] {
  let newline = (char newline)
  let original = (open --raw $main_rs)
  let module = (open --raw $corpus_js)
  let embedded = ($module | str replace 'export async function runInstalledEhCorpus' 'async function runInstalledEhCorpus')
  if $embedded == $module {
    error make {msg: 'corpus module export anchor missing'}
  }
  if ($embedded | str contains '"#') {
    error make {msg: 'Rust raw-string terminator in corpus module'}
  }

  let field_needle = ('    semantic_observed: Option<String>,\n    smoke_error: Option<String>,' | split row '\n' | str join $newline)
  let field_replacement = ('    semantic_observed: Option<String>,\n    corpus_observed: Option<String>,\n    smoke_error: Option<String>,' | split row '\n' | str join $newline)
  let step1 = (replace_once $original $field_needle $field_replacement)
  let receipt_needle = ('            "semanticObserved": result.semantic_observed.as_deref().unwrap_or("").chars().take(4096).collect::<String>(),\n' | split row '\n' | str join $newline)
  let receipt_replacement = ('            "semanticObserved": result.semantic_observed.as_deref().unwrap_or("").chars().take(4096).collect::<String>(),\n            "corpusObserved": result.corpus_observed.as_deref().unwrap_or("").chars().take(32768).collect::<String>(),\n' | split row '\n' | str join $newline)
  let step2 = (replace_once $step1 $receipt_needle $receipt_replacement)
  let script_insert = ('  ' + $embedded + $newline + "  window.addEventListener('load', () => setTimeout(async () => {")
  let step3 = (replace_once $step2 "  window.addEventListener('load', () => setTimeout(async () => {" $script_insert)
  let old = 'semantic.sameConnection && semantic.recovery && semantic.targetedPathsOk;'
  let new = ('semantic.sameConnection && semantic.recovery && semantic.targetedPathsOk;\nconst corpus = await runInstalledEhCorpus(bridge);\nresult.corpusObserved = JSON.stringify(corpus);\nresult.ok = result.ok && corpus.passed;' | split row '\n' | str join $newline)
  let step4 = (replace_once $step3 $old $new)
  $step4 | save --force $main_rs
}
