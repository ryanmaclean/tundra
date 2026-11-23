# Selected-source third-party bill candidate

This bill covers 439 unique selected third-party Rust packages: 434 in Linux default and 439 in Linux sounds. Most rows bind legal text to a checksum-verified crate archive, a pinned upstream revision, or an existing exact exception approval. `eventsource-stream@0.2.3` and `http-serde@2.1.1` have no legal file in their exact archives or pinned source trees. Both declare an Apache-2.0 option, so their rows explicitly select that option and ship the byte-pinned official Apache-2.0 text as `standard-apache`, never as a purported crate file. The generated bill and installed `.deb` still require a hosted check before distribution is accepted.

Each `entries[]` row must identify one selected package by exact `package_id` (`source#name@version`), `name`, `version`, `source`, `Cargo.lock` `lock_checksum`, the inventory's `selected_license_ids`, and nonempty `texts[]`. Its `profiles` is a nonempty subset of the two supported Linux profiles. The generator reconciles each membership against **both** independently resolved exact-head inventories: common packages have both profiles, sounds-only packages only `x86_64-unknown-linux-gnu/tauri-sounds`; an unknown profile or a row unselected in its declared profile fails. New source texts use a safe path under `app/tauri/third-party-licenses/selected/`, its SHA-256, and `source_kind: "crate"` plus an exact `crate_member` within that checksum-verified crate archive. Existing exception texts instead use `source_kind: "verified-exception"` and their existing path; the generator requires an exact matching entry in the nine-text exception manifest. For archives omitting a root license file, `vcs-root` entries record SHA-pinned upstream GitHub legal text and compare its commit to `.cargo_vcs_info.json` inside the verified crate. The `serial@0.4.0` `vcs-tag` row checks the annotated `v0.4.0` tag's peeled commit and compares **all six** archived crate members byte-for-byte to the corresponding files in that commit's `serial/` subtree, as well as its root license digest. The existing exception manifest and approval policy remain separate and strict.

Example row shape (values are illustrative; this is **not** a source-verified entry):

```json
{
  "package_id": "registry+https://github.com/rust-lang/crates.io-index#example@1.2.3",
  "name": "example",
  "version": "1.2.3",
  "source": "registry+https://github.com/rust-lang/crates.io-index",
  "lock_checksum": "<64 lowercase hex from Cargo.lock>",
  "profiles": ["x86_64-unknown-linux-gnu/default", "x86_64-unknown-linux-gnu/tauri-sounds"],
  "selected_license_ids": ["MIT"],
  "texts": [{
    "file": "app/tauri/third-party-licenses/selected/example-1.2.3/LICENSE",
    "text_sha256": "<64 lowercase hex of exact file>",
    "source_kind": "crate",
    "crate_member": "example-1.2.3/LICENSE"
  }]
}
```

The hosted generator reconciles the entire selected third-party product set, rejects missing/stale/duplicate package IDs, validates each lock/archive checksum and local text hash, compares crate-member or pinned-upstream bytes, then writes a profile-bound `selected-notice-bill.json` for bundling. A package with no supported legal-text provenance hard-fails. For the two `standard-apache` rows it checks the exact name, version, registry source, archive checksum, Cargo license declaration and author, and the official Apache text hash; it additionally checks `eventsource-stream`'s README declaration. This is a distribution license choice under each crate's `OR` expression, not a claim that either upstream archive contains that text. The declared authors are attribution data, not proven copyright holders. The `.deb` job must compare the generated bill and every referenced legal text against extracted installed bytes before retaining an approved artifact.

This only closes the selected Rust source-text path once populated. Full distribution review must separately establish actual compiled/copied contribution, copyright/attribution obligations for base licenses, UI `dist` and vendored JS/font assets, native code, external runtime dependencies, and each other platform/format/feature artifact. The `license_inventory.nu` `notice_ids` field only marks selected non-base license atoms; it is not a complete bill by itself.
