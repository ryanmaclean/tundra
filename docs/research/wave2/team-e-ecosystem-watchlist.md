# Team E — Ecosystem Watchlist (2026-02-23)

Purpose: track external Rust ecosystem bets requested by product direction and map each to an execution decision (`adopt`, `pilot`, `watch`, `defer`).

## Scope

### Editor / UX platforms
- [Lapce](https://github.com/lapce/lapce)
- [Redox OS](https://www.redox-os.org/)
- [Codex CLI (open source)](https://github.com/openai/codex)

### Concurrency / runtime model
- [crossbeam](https://github.com/crossbeam-rs/crossbeam)
- [rayon](https://github.com/rayon-rs/rayon) (including WebAssembly viability)

### Audio / interaction
- [rodio](https://github.com/RustAudio/rodio) (Balatro-mode sound cues)

### Shell / terminal substrate
- [nushell](https://github.com/nushell/nushell)
- zellij sidecar (already tracked in Wave 2 Team D notes)

### Remote control / multi-operator
- [rustdesk](https://github.com/rustdesk/rustdesk)

### Memory / agent context
- [cortex-mem](https://github.com/sopaco/cortex-mem)
- Graphiti memory (existing in product)

### Agent framework overlap / orchestration
- [rig](https://github.com/0xplaygrounds/rig)
- `claude-sdk-rs` lifecycle mapping (existing Wave 2 Team C track)

### Git / workflow / backups
- [git2-rs](https://github.com/rust-lang/git2-rs)
- [rustic](https://github.com/rustic-rs/rustic)
- [persistent-scheduler](https://github.com/rustmailer/persistent-scheduler)

### Local inference / model runtime
- [vllm.rs](https://github.com/guoqingbao/vllm.rs)
- [candle](https://github.com/huggingface/candle)

### Native macOS UX / HIG tooling
- [Rust Search (Xcode helper)](https://github.com/ParthJadhav/Rust_Search)

### Data plane
- DuckDB WASM (front-end OLAP lane, separate from SQLite OLTP lane)

## Decision Matrix

| Item | Decision | Why | Next concrete step |
|------|----------|-----|--------------------|
| git2-rs | `pilot` | safer read-path than shell parsing | migrate additional read-paths in `at-core` + parity fixtures |
| rustic | `watch` | backup fit exists, but broad dependency/runtime surface | prototype optional `at-backup` crate only if backup demand increases |
| persistent-scheduler | `pilot` | matches queue/refinery recurring jobs | benchmark against current scheduler + define persistence contract |
| vllm.rs | `pilot` | local inference failover with OpenAI-compatible serving | add `ProviderKind::Local` profile bootstrap path and health probe |
| candle | `watch` | strong long-term runtime, but larger embed complexity | keep as feasibility track behind vllm.rs serving first |
| Lapce | `watch` | good UX signals for Rust-native editor shell | monthly feature diff audit for native shell roadmap |
| Redox OS | `watch` | architecture inspiration, not immediate dependency | track kernel/process model ideas for isolation docs |
| crossbeam | `adopt` | strong fit with queue/event bus patterns | identify hot paths still using mutex-heavy channels |
| rayon | `defer` (WASM), `pilot` (native) | WASM threading constraints still uneven | use native-only hotspots first; gate WASM usage behind feature checks |
| rodio | `pilot` | easy win for audible state cues in Balatro mode | add optional Tauri sound pack + accessibility toggle |
| nushell | `watch` | shell embedding possible, high integration cost | evaluate as optional terminal profile, not default substrate |
| rustdesk | `defer` | heavy scope, security/compliance cost | revisit for enterprise remote-ops edition only |
| cortex-mem | `watch` | overlaps current memory architecture | compare with Graphiti + existing memory APIs before adopting |
| codex CLI frontend reuse | `watch` | legal/UX potential, integration unknowns | perform adapter spike via CLI bridge prototype |
| rig | `watch` | overlaps in orchestration abstractions | map overlap with `at-agents` before any adoption |
| Rust_Search | `watch` | useful native toolchain companion | evaluate integration as optional external tool link |
| DuckDB WASM | `pilot` | strong OLAP-in-UI fit | start analytics page prototype; keep SQLite for OLTP |

## Research TODO Backlog

1. Produce `pilot` acceptance criteria per item (perf, complexity, rollback).
2. Add a monthly watch cadence doc for `watch` items with signal thresholds.
3. Create `native-only` vs `wasm-safe` policy for concurrency/audio dependencies.
4. Define security model for any remote-control/embedding candidates.
5. Add one benchmark harness for data-plane decisions (SQLite vs DuckDB WASM).
6. Add one benchmark harness for inference lane (cloud vs local provider failover).

## Owner Mapping

- Team A (Data): DuckDB WASM, scheduler, benchmark harness.
- Team B (Git): git2-rs, rustic backup feasibility.
- Team C (Inference/Agents): vllm.rs, candle, rig/codex lifecycle overlap.
- Team D (Native UX): Lapce/Redox watchlist, rodio, Nushell, Rust Search.
- Security lane (cross-cutting): rustdesk/cortex-mem risk assessment.
