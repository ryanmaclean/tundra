# Target-specific dependency license inventory

`strict-licenses.yml` resolves the locked Cargo graph separately for
`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`, and the shipped `wasm32-unknown-unknown` UI.
These are the declared inventory targets; other architectures are not
implicitly covered. `scripts/license_inventory.nu`
starts at the shipped binary/UI crates and traverses normal and build edges.
Build dependencies stay in the product set because generated code can enter a
shipped artifact. Dev-only dependencies and unselected workspace packages are
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
satisfy a branch. `WITH` expressions remain review-required. MPL, CC0, Zlib,
and BSL-1.0 are not approved product licenses. Tooling licenses are not a
product gate. The all-target,
all-feature cargo-deny job remains a nonblocking diagnostic, so its failures
cannot be mistaken for shipped-product violations.

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
| `Apache-2.0 WITH LLVM-exception` | review required | `WITH` is unsupported |

The matrix covers default features on all four targets and the opt-in
`at-tauri/sounds` feature on the three native targets. Each JSON artifact
records its target, profile, and exact Cargo feature arguments, and its name
includes the target and profile. Other release feature combinations need their
own inventory before shipping. The JSON artifacts bind findings to package
versions, sources, manifest paths, target, and roots. A passing
declared-license inventory is not itself a license-file audit or a notice
bundle; release owners must retain exact license texts for notice-required
packages.
