# Optional file watcher

`at-core` keeps filesystem watching behind its `file-watcher` feature. The
default feature set does not include `notify` or `crossbeam-channel` through
`at-core`; no production workspace crate currently calls the watcher API.

Consumers that need watching can enable the feature on their `at-core`
dependency:

```toml
at-core = { path = "../at-core", features = ["file-watcher"] }
```

The API remains available as `at_core::file_watcher` when enabled. This feature
does not change the workspace-level `notify` declaration or the other default
features of `at-core`. An `--all-features` dependency audit still includes
`notify`; judge the shipped/default graph separately from opt-in features.

The strict-license workflow retains a Linux `at-core/file-watcher` Cargo
resolve graph and runs the opt-in watcher module tests on hosted CI. Its
default-profile inventory artifacts independently show the feature disabled.
This opt-in proof does not add the watcher to a shipped profile or validate
watcher behavior on macOS, Windows, or BSD.
