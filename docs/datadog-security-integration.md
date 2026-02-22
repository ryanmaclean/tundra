# Datadog Security Integration Guide

## Current State

You're using **Semgrep** for Rust static analysis (which is correct — Datadog SAST doesn't support Rust). Datadog can ingest Semgrep results via SARIF format.

## Options

### ✅ Option 1: Upload Semgrep SARIF to Datadog (Recommended)

**Why:** Keep your custom Rust rules in Semgrep, get unified reporting in Datadog.

**How to integrate:**

1. **Update `.github/workflows/ci.yml`** — Add after the existing Semgrep step:

```yaml
      - name: Semgrep SAST (SARIF output)
        if: always()
        run: semgrep --config .semgrep.yml --output semgrep-results.sarif --sarif crates/

      - name: Upload Semgrep results to Datadog
        if: always() && github.event_name == 'push' && github.ref == 'refs/heads/main'
        env:
          DD_API_KEY: ${{ secrets.DATADOG_API_KEY }}
          DD_APP_KEY: ${{ secrets.DATADOG_APP_KEY }}
          DD_SITE: ${{ secrets.DATADOG_SITE || 'datadoghq.com' }}
        run: |
          npm install -g @datadog/datadog-ci
          datadog-ci sarif upload semgrep-results.sarif
```

2. **Required secrets:**
   - `DATADOG_API_KEY` (already set for JUnit upload)
   - `DATADOG_APP_KEY` (needs `code_analysis_read` scope)
   - `DATADOG_SITE` (optional, defaults to `datadoghq.com`)

3. **Benefits:**
   - All security findings in one place (Datadog Code Security)
   - Service/team attribution via CODEOWNERS
   - PR comments (if GitHub integration configured)
   - Historical tracking and trending

### Option 2: Use Datadog SAST for Non-Rust Code

**When:** If you add Python/JS/Go/etc. later, use Datadog SAST for those languages.

**Setup:** See [Datadog SAST Setup](https://docs.datadoghq.com/security/code_security/static_analysis/setup/)

**Note:** Your codebase is primarily Rust, so this isn't applicable now.

### Option 3: Datadog IAST (Runtime Analysis)

**What:** Interactive Application Security Testing — detects vulnerabilities during runtime.

**Use case:** Complementary to SAST, not a replacement. Useful for:
- Detecting SQL injection, XSS, path traversal in running services
- API security testing
- Dependency vulnerabilities in production

**Setup:** Requires APM agent instrumentation. See [IAST Setup](https://docs.datadoghq.com/security/code_security/iast/)

**Note:** Since you're using OpenTelemetry (not Datadog APM), IAST integration would require switching or dual-instrumentation.

## Recommendation

**Keep Semgrep + Upload to Datadog (Option 1)** because:
1. ✅ Datadog SAST doesn't support Rust
2. ✅ Your custom Semgrep rules are Rust-specific and well-tuned
3. ✅ Semgrep SARIF upload is tested/supported by Datadog
4. ✅ Unified reporting in Datadog Code Security dashboard
5. ✅ No need to rewrite rules or switch tools

## SARIF Format Notes

Semgrep's SARIF output includes:
- Rule IDs matching your `.semgrep.yml` rules
- CWE mappings (already in your metadata)
- Severity levels (ERROR/WARNING map to Critical/High in Datadog)
- File locations and code snippets

Your current rules will map correctly:
- `unsafe-block-without-safety-comment` → Security category
- `unwrap-in-lib` → Code Quality category
- `command-injection-risk` → Security (Critical)
- `path-join-unsanitized` → Security (High)
- `hardcoded-secret` → Security (Critical)

## Next Steps

1. Add the SARIF upload step to CI (see code above)
2. Ensure `DATADOG_APP_KEY` has `code_analysis_read` scope
3. Run a test scan on main branch
4. Verify results appear in [Datadog Code Security](https://app.datadoghq.com/security/configuration/code-security/setup)

## References

- [Datadog SAST Third-Party Upload](https://docs.datadoghq.com/security/code_security/static_analysis/setup/?tab=circleciorbs#upload-third-party-static-analysis-results-to-datadog)
- [Semgrep SARIF Output](https://semgrep.dev/docs/cli-reference/#sarif-output)
- [datadog-ci CLI](https://github.com/DataDog/datadog-ci)
