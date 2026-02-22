# Troubleshooting Guide

Common issues and solutions for the Agent Harness.

## 📚 Quick Navigation

- [API Key Issues](#api-key-issues)
- [Model Availability](#model-availability)
- [Quota Problems](#quota-problems)
- [Connection Errors](#connection-errors)
- [Build & Compilation](#build--compilation)
- [Runtime Errors](#runtime-errors)
- [Performance Issues](#performance-issues)

---

## API Key Issues

### ❌ "Invalid API key format"

**Symptom:**
```
❌ Error: Invalid API key format

API key must:
  - Start with 'sk-or-v1-' (OpenRouter prefix)
  - Be at least 20 characters long
  - Contain only alphanumeric characters, hyphens, and underscores
```

**Causes:**
1. Wrong API key format
2. Extra spaces or quotes in key
3. Using wrong provider's key
4. Environment variable not set

**Solutions:**

✅ **Check key format:**
```bash
echo $OPENROUTER_API_KEY
```
Should output: `sk-or-v1-xxxxxxxxxxxxx...`

✅ **Verify no extra characters:**
```bash
# Remove quotes if present
export OPENROUTER_API_KEY=sk-or-v1-your-key-here  # NO quotes!
```

✅ **Get a new key:**
1. Go to https://openrouter.ai/keys
2. Create new API key
3. Copy and set it:
```bash
export OPENROUTER_API_KEY=sk-or-v1-new-key-here
```

✅ **Check environment variable:**
```bash
# Should show your key
env | grep OPENROUTER

# If not set:
export OPENROUTER_API_KEY=your-key-here
```

**Prevention:**
- Store in `.env` file for persistence
- Never commit `.env` to git
- Verify key before running: `echo $OPENROUTER_API_KEY`

---

### ❌ "Authentication failed"

**Symptom:**
```
❌ API Error: 401 Unauthorized
Authentication failed
```

**Causes:**
1. Invalid or expired API key
2. Key doesn't have required permissions
3. Account suspended

**Solutions:**

✅ **Test your key:**
```bash
curl -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  https://openrouter.ai/api/v1/models
```

✅ **Check OpenRouter dashboard:**
1. Visit https://openrouter.ai/settings/keys
2. Verify key status
3. Check account standing

✅ **Generate new key:**
- Old key might be revoked
- Create fresh key in dashboard

**Prevention:**
- Monitor key usage
- Set up billing alerts
- Keep backup key for emergencies

---

## Model Availability

### ❌ "Model unavailable"

**Symptom:**
```
❌ Model unavailable: meta-llama/llama-3.3-70b-instruct:free
```

**Causes:**
1. Free model temporarily down
2. Model removed from OpenRouter
3. High demand on free tier
4. Regional restrictions

**Solutions:**

✅ **Try automatic fallback:**
The system should automatically suggest alternatives:
```
🔄 Attempting fallback to: arcee-ai/trinity-large-preview:free
```

✅ **Check OpenRouter status:**
- Visit https://status.openrouter.ai
- Check model availability
- See if there are known issues

✅ **Use alternative models:**
```bash
# In interactive mode, the system auto-switches
# Or specify manually in code:
let config = ProviderConfig {
    model: "arcee-ai/trinity-large-preview:free".to_string(),
    // ...
};
```

✅ **Try different time:**
- Free models more available during off-peak hours
- US night time = better availability

**Prevention:**
- Have multiple models configured
- Use fallback strategy
- Monitor OpenRouter announcements

---

### ❌ "Model not found"

**Symptom:**
```
❌ Error: Model 'my-custom-model' not found
```

**Causes:**
1. Typo in model name
2. Model doesn't exist on OpenRouter
3. Model requires paid tier

**Solutions:**

✅ **List available models:**
```bash
curl -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  https://openrouter.ai/api/v1/models | jq '.data[].id'
```

✅ **Use known free models:**
- `meta-llama/llama-3.3-70b-instruct:free`
- `arcee-ai/trinity-large-preview:free`
- `deepseek/deepseek-r1-0528:free`

✅ **Check model documentation:**
- Visit https://openrouter.ai/models
- Verify exact model name
- Check if `:free` suffix needed

---

## Quota Problems

### ⚠️ "Approaching quota limit"

**Symptom:**
```
⚠️  Warning: Token quota at 85% (8,500/10,000)
```

**Causes:**
- Normal usage approaching daily limit

**Solutions:**

✅ **Check current usage:**
```bash
# In interactive mode:
status
```

✅ **Monitor and plan:**
```
📊 API QUOTA STATUS
============================================================
✅ 🆓 meta-llama/llama-3.3-70b-instruct:free
  Requests: 85/100 (85.0%)
  Tokens: 8,500/10,000 (85.0%)
  Reset: 2026-02-16 00:00:00 UTC (in 2h 15m)
============================================================
```

✅ **Strategies:**
- Wait for reset (midnight UTC)
- Use shorter prompts
- Switch to different free model
- Upgrade to paid tier

**Prevention:**
- Check `status` regularly
- Keep prompts concise
- Use multiple models strategically

---

### ❌ "Quota exceeded"

**Symptom:**
```
❌ Error: Quota exceeded for meta-llama/llama-3.3-70b-instruct:free
   Requests: 100/100 (100%)
   Reset: 2026-02-16 00:00:00 UTC (in 5h 23m)
```

**Causes:**
- Hit daily limit (100 requests or 10k tokens)

**Solutions:**

✅ **Wait for reset:**
- Quota resets at midnight UTC
- Check reset time in error message

✅ **Use alternative model:**
```
💡 Suggestions:
   - Try alternative model: arcee-ai/trinity-large-preview:free
```

✅ **Upgrade account:**
- Visit https://openrouter.ai/settings/credits
- Add credits for higher limits
- Pay-per-use or subscription

✅ **Optimize usage:**
- Batch related questions
- Use shorter system prompts
- Clear unnecessary context

**Prevention:**
- Monitor quota throughout the day
- Spread usage across multiple models
- Set up usage alerts
- Plan high-volume tasks for after reset

---

## Connection Errors

### ❌ "Connection timeout"

**Symptom:**
```
❌ Connection error: Request timed out after 30s
```

**Causes:**
1. No internet connection
2. Firewall blocking requests
3. OpenRouter API slow/down
4. Network proxy issues

**Solutions:**

✅ **Check internet:**
```bash
ping google.com
curl https://openrouter.ai
```

✅ **Test OpenRouter directly:**
```bash
curl -I https://openrouter.ai/api/v1/models
```

✅ **Check firewall:**
- Allow outbound HTTPS (port 443)
- Whitelist `openrouter.ai`
- Check corporate proxy settings

✅ **Increase timeout (in code):**
```rust
let provider = ProviderConfig {
    timeout: Duration::from_secs(60), // Increase from 30s
    // ...
};
```

**Prevention:**
- Use stable internet connection
- Configure proxy if needed
- Monitor OpenRouter status

---

### ❌ "DNS resolution failed"

**Symptom:**
```
❌ Connection error: DNS resolution failed for openrouter.ai
```

**Causes:**
1. DNS server issues
2. Network configuration problem
3. Local DNS cache corruption

**Solutions:**

✅ **Test DNS:**
```bash
nslookup openrouter.ai
dig openrouter.ai
```

✅ **Flush DNS cache:**
```bash
# macOS
sudo dscacheutil -flushcache
sudo killall -HUP mDNSResponder

# Linux
sudo systemd-resolve --flush-caches

# Windows
ipconfig /flushdns
```

✅ **Use alternative DNS:**
```bash
# Temporarily use Google DNS
# macOS: System Preferences → Network → DNS
# Add: 8.8.8.8, 8.8.4.4
```

---

## Build & Compilation

### ❌ "Cargo build failed"

**Symptom:**
```
error: failed to compile agent-harness v0.1.0
```

**Causes:**
1. Wrong Rust version
2. Missing dependencies
3. Corrupted cargo cache
4. Network issues downloading deps

**Solutions:**

✅ **Check Rust version:**
```bash
rustc --version  # Should be 1.91+

# Update if needed:
rustup update
```

✅ **Clean and rebuild:**
```bash
cargo clean
cargo build
```

✅ **Update dependencies:**
```bash
cargo update
cargo build
```

✅ **Check Cargo.lock:**
```bash
# Remove lock file and regenerate
rm Cargo.lock
cargo build
```

✅ **Clear cargo cache (last resort):**
```bash
cargo clean
rm -rf ~/.cargo/registry
cargo build
```

**Prevention:**
- Keep Rust updated: `rustup update`
- Commit `Cargo.lock` to git
- Use stable Rust channel

---

### ❌ "Linking errors"

**Symptom:**
```
error: linking with `cc` failed: exit status: 1
```

**Causes:**
1. Missing system libraries
2. Incompatible C toolchain
3. SQLite bundled feature issues

**Solutions:**

✅ **Install system dependencies:**
```bash
# macOS
xcode-select --install

# Ubuntu/Debian
sudo apt-get install build-essential libssl-dev pkg-config

# Fedora
sudo dnf install gcc openssl-devel
```

✅ **Check SQLite bundled feature:**
The project uses `rusqlite = { version = "0.32", features = ["bundled"] }`
This should work without system SQLite, but if issues persist:
```bash
# macOS
brew install sqlite

# Ubuntu/Debian
sudo apt-get install libsqlite3-dev
```

---

### ⚠️ "Clippy warnings"

**Symptom:**
```
warning: unused variable: `x`
```

**Solutions:**

✅ **Fix warnings:**
```bash
cargo clippy --fix
```

✅ **Check all warnings:**
```bash
cargo clippy -- -W clippy::all
```

**Note:** Warnings don't prevent compilation, but fixing them improves code quality.

---

## Runtime Errors

### ❌ "Thread panicked"

**Symptom:**
```
thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value'
```

**Causes:**
1. Unhandled error
2. Network failure
3. Invalid data format

**Solutions:**

✅ **Enable backtraces:**
```bash
RUST_BACKTRACE=1 cargo run --bin interactive
```

✅ **Check logs:**
```bash
# Enable debug logging
RUST_LOG=debug cargo run --bin interactive
```

✅ **Review stack trace:**
- Identifies exact error location
- Shows call chain
- Points to problematic code

✅ **Report issue:**
If persistent, file a bug report with:
- Full error message
- Backtrace output
- Steps to reproduce

---

### ❌ "JSON parsing error"

**Symptom:**
```
Error: Failed to parse JSON response
```

**Causes:**
1. Malformed API response
2. Unexpected response format
3. OpenRouter API change

**Solutions:**

✅ **Enable debug logging:**
```bash
RUST_LOG=debug cargo run --bin interactive
```
This shows raw API responses.

✅ **Check OpenRouter status:**
- API might be returning errors
- Format might have changed

✅ **Retry request:**
- Might be temporary glitch
- Try different model

**Prevention:**
- Keep dependencies updated
- Monitor OpenRouter changelog
- Use latest agent-harness version

---

## Performance Issues

### ⏱️ "Slow responses"

**Symptom:**
- Requests take >5 seconds
- UI feels sluggish

**Causes:**
1. Slow LLM model
2. Large context (many messages)
3. Network latency
4. Complex tool calls

**Solutions:**

✅ **Use faster models:**
```rust
// Switch to smaller/faster model
model: "meta-llama/llama-3.3-70b-instruct:free"  // Fast
// vs
model: "some-larger-model"  // Slower
```

✅ **Prune conversation history:**
```rust
use agent_harness::memory_management::{PruningMemory, PruningStrategy};

let pruning = PruningMemory::new(
    memory,
    max_tokens: 2000,  // Reduce context
    PruningStrategy::Recent
);
```

✅ **Check network:**
```bash
# Test latency to OpenRouter
curl -w "@curl-format.txt" -o /dev/null -s https://openrouter.ai
```

✅ **Enable streaming (in code):**
```rust
// Streaming provides incremental responses
provider.chat_completion_stream(messages, tools).await?;
```

---

### 💾 "High memory usage"

**Symptom:**
- Agent-harness using >500MB RAM
- System becomes slow

**Causes:**
1. Too many conversations in memory
2. Long conversation histories
3. Large message content

**Solutions:**

✅ **Use persistent storage:**
```rust
// SQLite instead of in-memory
let memory = SqliteMemory::new(PathBuf::from("./data/conv.db")).await?;
```

✅ **Implement pruning:**
```rust
let pruning = PruningMemory::new(
    memory,
    max_tokens: 4000,
    PruningStrategy::Importance
);
```

✅ **Clear old conversations:**
```rust
memory.clear("old-conversation-id").await?;
```

---

## Still Having Issues?

### 🐛 Report a Bug

If you've tried everything and still have problems:

1. **Gather information:**
   ```bash
   # System info
   rustc --version
   cargo --version
   uname -a  # or `ver` on Windows

   # Error details
   RUST_BACKTRACE=full RUST_LOG=debug cargo run --bin interactive 2>&1 | tee error.log
   ```

2. **Check existing issues:**
   - Search GitHub issues
   - Look for similar problems

3. **Create minimal reproduction:**
   - Simplest code that shows the problem
   - Steps to reproduce

4. **Include in report:**
   - Error message
   - Backtrace
   - System info
   - Steps to reproduce
   - Expected vs actual behavior

### 📚 Additional Resources

- **Architecture Guide**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Getting Started**: [GETTING_STARTED.md](GETTING_STARTED.md)
- **Examples**: [EXAMPLES.md](EXAMPLES.md)
- **OpenRouter Docs**: https://openrouter.ai/docs
- **Rust Error Handling**: https://doc.rust-lang.org/book/ch09-00-error-handling.html

---

## Quick Fixes Checklist

When something goes wrong, try these in order:

- [ ] Check API key is set: `echo $OPENROUTER_API_KEY`
- [ ] Verify internet connection: `ping google.com`
- [ ] Check OpenRouter status: https://status.openrouter.ai
- [ ] Try different model
- [ ] Clean and rebuild: `cargo clean && cargo build`
- [ ] Update Rust: `rustup update`
- [ ] Check quota: `status` command in interactive mode
- [ ] Enable debug logging: `RUST_LOG=debug`
- [ ] Check recent OpenRouter API changes
- [ ] Restart terminal session

**Still stuck?** Review the specific error section above or check the architecture guide to understand the system better.
