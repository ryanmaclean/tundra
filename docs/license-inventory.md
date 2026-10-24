# Target-specific dependency license inventory

`strict-licenses.yml` resolves the locked Cargo graph separately for
`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`, and the shipped `wasm32-unknown-unknown` UI.
These are the declared inventory targets; other architectures are not
implicitly covered. `scripts/license_inventory.nu`
starts at the shipped binary/UI crates and traverses normal and build edges.
An optional edge is skipped only when its target package and source map to one
optional manifest declaration and no resolved parent feature activates it.
Unknown or ambiguous mappings remain included. The JSON records every skipped
edge and its parent features for review. Paths that enter a build edge are
reported as `build-only` unless a normal path also reaches that package;
build-only licenses are tooling-approved, while generated or copied build
output still needs release review. Dev-only dependencies and unselected workspace packages are
listed separately as `tool-only-or-unselected`; this name deliberately does not
assert that every unselected package is a tool on every release target. Only
the fourteen reviewed workspace package names listed in the script and present
in Cargo metadata's `workspace_members` receive the private first-party
exemption; arbitrary path dependencies do not.

Product packages pass only when a syntactically valid SPDX `AND`/`OR`
expression has an approved satisfying license choice. Approved IDs are MIT,
BSD-2-Clause, BSD-3-Clause, Apache-2.0, and the Tundra-approved exact terms
ISC, CDLA-Permissive-2.0, and Unicode-3.0. Parentheses and `AND` precedence
are honored. The chosen terms appear in `selected_license_ids`; the latter
three appear in `notice_ids` for release notice assembly. Unknown IDs cannot
satisfy a branch. A simple `license-ID WITH exception-ID` is parsed as one
unapproved atom; an independent plain approved `OR` branch may satisfy the
expression. A mandatory `WITH` atom, grouped `WITH` expression, repeated
`WITH`, or malformed operand is review-required. This does not approve any
exception. MPL, CC0, Zlib, and BSL-1.0 are not approved product licenses.
Tooling licenses are not a product gate. The all-target,
all-feature cargo-deny job remains a nonblocking diagnostic, so its failures
cannot be mistaken for shipped-product violations.

Current additional product approvals are in `scripts/license_approvals.json`.
Each record binds a decision reference to one exact Cargo package ID, version,
source, `Cargo.lock` checksum, declared expression, selected satisfying choice,
notice IDs, and target/feature profiles. Duplicate, stale, unused, changed, or
out-of-profile records fail the gate. An empty approval list grants nothing.
The inventory records the approval manifest SHA-256 alongside its metadata
and lockfile hashes.
The staged manifest contains ten exact tuples from the bf3 candidate roster:
five MPL, four Zlib, and one Boost `BSL-1.0`. It excludes
`finl_unicode`, `wezterm-bidi`, and `terminfo` because c45 metadata shows the
Ratatui Termwiz feature inactive, and excludes build-only `target-lexicon`.
The likely build-only `base16` and `const_format_proc_macros` are also
deferred until exact WASM profile role readback.
The new traversal and approval use require exact-head hosted readback before
publication.

Cargo packages can declare older slash-separated alternatives. Only a plain
slash-separated list of license IDs is interpreted as alternatives; it then
passes through the same complete grammar and policy evaluation. A declaration
that mixes `/` with `AND`, `OR`, `WITH`, or parentheses is review-required
because its precedence is ambiguous. The inventory preserves the original
declaration and marks `legacy_slash_alternatives` in JSON. Empty alternatives,
unexpected punctuation, and unknown IDs also remain review-required unless a
well-formed slash-only declaration has another approved choice.

Examples of policy evaluation:

| Declared SPDX expression | Result | Chosen terms / notices |
| --- | --- | --- |
| `(Apache-2.0 OR ISC OR MIT-0)` | allowed | `Apache-2.0`; no extra notice |
| `MIT/Apache-2.0` or `Apache-2.0 / MIT` | allowed | `MIT` or `Apache-2.0`; no extra notice |
| `Unlicense/MIT` | allowed | `MIT`; Unlicense branch unselected |
| `MIT//Apache-2.0` | review required | Empty slash alternative is invalid |
| `MIT/Apache-2.0 AND MPL-2.0` | review required | Mixed slash and SPDX operator |
| `MIT AND (MPL-2.0/Zlib)` | review required | Mixed slash, operator, and parentheses |
| `ISC AND (Apache-2.0 OR ISC)` | allowed | `ISC`, `Apache-2.0`; ISC notice |
| `Apache-2.0 AND MPL-2.0` | review required | No complete approved choice |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | allowed | Plain `Apache-2.0` branch; no exception approval |
| `Apache-2.0 WITH LLVM-exception` | review required | Sole branch has unapproved compound license |
| `MIT AND Apache-2.0 WITH LLVM-exception` | review required | Mandatory compound branch |
| `MIT OR GPL-2.0-only WITH Classpath-exception-2.0` | allowed | Plain `MIT` branch; no GPL/Classpath approval |
| `(MIT OR Apache-2.0) WITH LLVM-exception` | review required | Grouped `WITH` is invalid |
| `MIT WITH LLVM-exception WITH Other-exception OR Apache-2.0` | review required | Repeated `WITH` is invalid |
| `MIT WITH OR Apache-2.0` | review required | Missing exception ID |

The matrix covers default features on all four targets and the opt-in
`at-tauri/sounds` feature on the three native targets. Each JSON artifact
records its target, profile, and exact Cargo feature arguments, and its name
includes the target and profile. Other release feature combinations need their
own inventory before shipping. The JSON artifacts bind findings to package
versions, sources, manifest paths, target, and roots. Each hosted artifact also
retains the exact `cargo-metadata.json` used for that inventory, including
`resolve.nodes[].features` and each dependency's `dep_kinds`. A reviewer can
follow a flagged package's ID through `resolve.nodes[].deps` to see whether a
native dependency such as Termwiz entered via a normal or build edge and which
Ratatui features were selected. The inventory records SHA-256 hashes for that
raw metadata file and `Cargo.lock`; artifacts are compressed and retained for
14 days. No credentials are passed to `cargo metadata`, and the checked
manifests contain no credential-bearing Git dependency URLs. A passing
declared-license inventory is not itself a license-file audit or a notice
bundle; release owners must retain exact license texts for notice-required
packages.
