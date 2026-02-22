# Migration from Semgrep to ast-grep

## Summary

We've migrated from **Semgrep** (LGPL 2.1) to **ast-grep** (MIT) for Rust static analysis security scanning.

## Why ast-grep?

- ✅ **MIT License** - Fully open source, permissive license
- ✅ **Written in Rust** - Native performance, no Python dependency
- ✅ **SARIF Support** - Can upload results to Datadog Code Security
- ✅ **AST-based Pattern Matching** - Similar expressiveness to Semgrep
- ✅ **Active Development** - 12k+ GitHub stars, actively maintained

## What Changed

### Configuration File

- **Old:** `.semgrep.yml` (Semgrep format)
- **New:** `.ast-grep.yml` (ast-grep YAML format)

### Rules Converted

All 5 Semgrep rules have been converted:

1. ✅ **Unsafe blocks without safety comments** - Detects `unsafe { }` without preceding `// SAFETY:` comment
2. ✅ **Unwrap/expect in library code** - Flags `.unwrap()` and `.expect()` outside test modules
3. ✅ **Command injection risks** - Detects `Command::new("sh")` or `Command::new("bash")` with `.arg()`
4. ✅ **Path traversal** - Flags `Path::new(...).join()` operations
5. ✅ **Hardcoded secrets** - Detects API keys/tokens in string literals (sk-*, ghp_*, Bearer *)

### Installation

**Old:**
```bash
pip3 install semgrep
```

**New:**
```bash
cargo install ast-grep --locked
# Or via Makefile:
make ast-grep-install
```

### Usage

**Old:**
```bash
semgrep --config .semgrep.yml crates/
make semgrep
```

**New:**
```bash
ast-grep scan --config .ast-grep.yml --exclude '**/test*.rs' crates/
make ast-grep
```

### CI Integration

The GitHub Actions workflow now:
1. Installs ast-grep via `cargo install`
2. Runs security scan with test file exclusions
3. Generates SARIF output for Datadog upload
4. Uploads results to Datadog Code Security (if configured)

## Rule Differences

### File Path Filtering

**Semgrep:** Supported `paths.include` and `paths.exclude` in rule config.

**ast-grep:** Uses CLI `--exclude` flags instead. Test files are excluded via:
```bash
--exclude '**/test*.rs' --exclude '**/*_test.rs'
```

### Pattern Matching

Both tools use AST-based patterns, but syntax differs:

**Semgrep:**
```yaml
pattern: unsafe { ... }
pattern-not-inside: // SAFETY: ...
```

**ast-grep:**
```yaml
pattern: unsafe { $$$ }
not:
  precedes:
    pattern: // SAFETY: $_
```

### Regex Matching

ast-grep uses Rust regex syntax (similar to Perl-style). Some patterns needed adjustment:
- String literal matching uses `has` + `regex` instead of direct regex on pattern
- Single quote escaping: `"^'sk-[^']*'$"` instead of `'^\'sk-[^\']*\'\'$'`

## SARIF Output

Both tools support SARIF format for Datadog integration:

**Semgrep:**
```bash
semgrep --config .semgrep.yml --output semgrep-results.sarif --sarif crates/
```

**ast-grep:**
```bash
ast-grep scan --config .ast-grep.yml --format sarif --output ast-grep-results.sarif crates/
```

## Benefits

1. **No Python Dependency** - Pure Rust tool, faster installation
2. **Better License** - MIT vs LGPL 2.1
3. **Native Performance** - Compiled Rust binary
4. **Same Functionality** - All security rules preserved
5. **Datadog Compatible** - SARIF upload works identically

## Future Enhancements

- Add more Rust-specific security rules
- Integrate with Clippy lints via `clippy-sarif` for unified reporting
- Consider custom dylint rules for project-specific patterns

## References

- [ast-grep GitHub](https://github.com/ast-grep/ast-grep)
- [ast-grep Documentation](https://ast-grep.github.io/)
- [Datadog SARIF Upload](https://docs.datadoghq.com/security/code_security/static_analysis/setup/?tab=circleciorbs#upload-third-party-static-analysis-results-to-datadog)
