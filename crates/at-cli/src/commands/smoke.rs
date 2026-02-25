use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::{Child, Command};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ScriptPresence {
    webgpu_analytics: bool,
    poker_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct CueResult {
    ok: bool,
    cue: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct WebGpuResult {
    supported: Option<bool>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct BrowserSmokeResult {
    url: Option<String>,
    scripts_present: ScriptPresence,
    webgpu: WebGpuResult,
    audio_warmup: CueResult,
    audio_cue: CueResult,
    errors: Vec<String>,
}

struct StaticServer {
    child: Child,
}

impl StaticServer {
    fn start(dist_dir: &Path, host: &str, port: u16) -> anyhow::Result<Self> {
        if !dist_dir.exists() {
            bail!("dist directory not found: {}", dist_dir.display());
        }
        let child = Command::new("python3")
            .arg("-m")
            .arg("http.server")
            .arg(port.to_string())
            .arg("--bind")
            .arg(host)
            .current_dir(dist_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start python3 http.server")?;
        Ok(Self { child })
    }
}

impl Drop for StaticServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub async fn run(
    ui_url: &str,
    project_path: &str,
    headful: bool,
    serve_dist: bool,
    strict: bool,
    json_output: bool,
    out_path: Option<&str>,
) -> anyhow::Result<()> {
    let parsed = Url::parse(ui_url).context("invalid --ui-url")?;
    let host = parsed.host_str().unwrap_or("127.0.0.1").to_string();
    let port = parsed.port_or_known_default().unwrap_or(3001) as u16;

    let mut static_server = None;
    if serve_dist {
        let dist_dir = PathBuf::from(project_path).join("app/leptos-ui/dist");
        static_server = Some(StaticServer::start(&dist_dir, &host, port)?);
    }

    wait_for_http(ui_url, Duration::from_secs(15)).await?;
    let runtime_dir = ensure_playwright_runtime().await?;
    let smoke_script = write_smoke_script(&runtime_dir)?;
    let result = run_browser_smoke(&runtime_dir, &smoke_script, ui_url, !headful).await?;

    let mut failures = Vec::<String>::new();
    if !result.scripts_present.webgpu_analytics {
        failures.push("webgpuAnalytics bridge missing".to_string());
    }
    if !result.scripts_present.poker_audio {
        failures.push("pokerAudio bridge missing".to_string());
    }
    if result.webgpu.supported.is_none() {
        failures.push("WebGPU probe did not return `supported`".to_string());
    }
    if !result.audio_warmup.ok {
        failures.push(format!(
            "Audio warmup failed: {}",
            result
                .audio_warmup
                .error
                .clone()
                .unwrap_or_else(|| "unknown error".to_string())
        ));
    }
    if !result.audio_cue.ok {
        failures.push(format!(
            "Audio cue failed: {}",
            result
                .audio_cue
                .error
                .clone()
                .unwrap_or_else(|| "unknown error".to_string())
        ));
    }
    if !result.errors.is_empty() {
        failures.extend(result.errors.iter().cloned());
    }

    let payload = json!({
        "url": ui_url,
        "served_dist": serve_dist,
        "headless": !headful,
        "result": result,
        "failures": failures,
    });

    if json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("browser smoke report");
        println!("{}", "-".repeat(40));
        println!("URL: {ui_url}");
        println!("Served dist: {}", if serve_dist { "yes" } else { "no" });
        println!("Mode: {}", if headful { "headful" } else { "headless" });
        println!(
            "Bridges: webgpuAnalytics={} pokerAudio={}",
            result.scripts_present.webgpu_analytics, result.scripts_present.poker_audio
        );
        match result.webgpu.supported {
            Some(true) => println!("WebGPU: supported"),
            Some(false) => println!(
                "WebGPU: unsupported ({})",
                result
                    .webgpu
                    .error
                    .clone()
                    .unwrap_or_else(|| "no adapter".to_string())
            ),
            None => println!("WebGPU: invalid response"),
        }
        println!(
            "Audio warmup: {}{}",
            if result.audio_warmup.ok {
                "ok"
            } else {
                "failed"
            },
            result
                .audio_warmup
                .error
                .as_ref()
                .map(|e| format!(" ({e})"))
                .unwrap_or_default()
        );
        println!(
            "Audio cue: {}{}",
            if result.audio_cue.ok { "ok" } else { "failed" },
            result
                .audio_cue
                .error
                .as_ref()
                .map(|e| format!(" ({e})"))
                .unwrap_or_default()
        );
        println!("Failures: {}", failures.len());
        for failure in &failures {
            println!("  - {failure}");
        }
    }

    if let Some(path) = out_path {
        write_json_artifact(path, &payload)?;
    }

    drop(static_server);

    if strict && !failures.is_empty() {
        bail!("browser smoke failed ({} issues)", failures.len());
    }

    Ok(())
}

async fn wait_for_http(url: &str, timeout: Duration) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let start = Instant::now();

    loop {
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if start.elapsed() >= timeout {
            bail!("timed out waiting for {url}");
        }
        sleep(Duration::from_millis(250)).await;
    }
}

async fn ensure_playwright_runtime() -> anyhow::Result<PathBuf> {
    let runtime_dir = std::env::temp_dir().join("at-smoke-playwright");
    std::fs::create_dir_all(&runtime_dir)?;

    let pkg = runtime_dir.join("node_modules/playwright/package.json");
    if pkg.exists() {
        return Ok(runtime_dir);
    }

    let init = Command::new("npm")
        .arg("init")
        .arg("-y")
        .current_dir(&runtime_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("failed running npm init")?;
    if !init.success() {
        bail!("npm init failed in {}", runtime_dir.display());
    }

    let install = Command::new("npm")
        .arg("install")
        .arg("--silent")
        .arg("playwright@1.58.2")
        .current_dir(&runtime_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("failed running npm install playwright")?;
    if !install.success() {
        bail!("npm install playwright failed in {}", runtime_dir.display());
    }

    Ok(runtime_dir)
}

fn write_smoke_script(runtime_dir: &Path) -> anyhow::Result<PathBuf> {
    let script = runtime_dir.join("tundra-smoke.cjs");
    let body = r#"
const { chromium } = require('playwright');

(async () => {
  const smokeUrl = process.env.TUNDRA_SMOKE_URL || 'http://127.0.0.1:3001';
  const headlessEnv = (process.env.TUNDRA_SMOKE_HEADLESS || 'true').toLowerCase();
  const headless = headlessEnv !== 'false';

  const result = {
    url: smokeUrl,
    scriptsPresent: { webgpuAnalytics: false, pokerAudio: false },
    webgpu: { supported: null, error: null },
    audioWarmup: { ok: false, state: null, error: null },
    audioCue: { ok: false, cue: null, error: null },
    errors: [],
  };

  let browser = null;
  try {
    browser = await chromium.launch({ headless });
    const page = await browser.newPage();
    await page.goto(smokeUrl, { waitUntil: 'domcontentloaded', timeout: 60000 });
    await page.waitForTimeout(1200);

    result.scriptsPresent = await page.evaluate(() => ({
      webgpuAnalytics: !!globalThis.webgpuAnalytics,
      pokerAudio: !!globalThis.pokerAudio,
    }));

    if (result.scriptsPresent.webgpuAnalytics) {
      const raw = await page.evaluate(async () => {
        try {
          return await globalThis.webgpuAnalytics.run_probe_webgpu(64);
        } catch (e) {
          return JSON.stringify({ supported: false, error: String(e) });
        }
      });
      try {
        result.webgpu = JSON.parse(raw);
      } catch (e) {
        result.webgpu = { supported: null, error: 'parse failure: ' + String(e) };
      }
    }

    await page.mouse.click(20, 20);
    if (result.scriptsPresent.pokerAudio) {
      const warmRaw = await page.evaluate(async () => {
        try {
          return await globalThis.pokerAudio.warmup();
        } catch (e) {
          return JSON.stringify({ ok: false, error: String(e) });
        }
      });
      try {
        result.audioWarmup = JSON.parse(warmRaw);
      } catch (e) {
        result.audioWarmup = { ok: false, error: 'parse failure: ' + String(e) };
      }

      const cueRaw = await page.evaluate(async () => {
        try {
          return await globalThis.pokerAudio.play_cue('consensus');
        } catch (e) {
          return JSON.stringify({ ok: false, cue: 'consensus', error: String(e) });
        }
      });
      try {
        result.audioCue = JSON.parse(cueRaw);
      } catch (e) {
        result.audioCue = { ok: false, cue: 'consensus', error: 'parse failure: ' + String(e) };
      }
    }
  } catch (e) {
    result.errors.push(String(e));
  } finally {
    if (browser) {
      await browser.close();
    }
  }

  process.stdout.write(JSON.stringify(result));
})();
"#;
    std::fs::write(&script, body)?;
    Ok(script)
}

async fn run_browser_smoke(
    runtime_dir: &Path,
    script_path: &Path,
    ui_url: &str,
    headless: bool,
) -> anyhow::Result<BrowserSmokeResult> {
    let output = Command::new("node")
        .arg(script_path)
        .current_dir(runtime_dir)
        .env("TUNDRA_SMOKE_URL", ui_url)
        .env(
            "TUNDRA_SMOKE_HEADLESS",
            if headless { "true" } else { "false" },
        )
        .output()
        .await
        .context("failed to execute browser smoke script")?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("smoke script produced no output; stderr: {stderr}"));
    }
    let mut parsed: BrowserSmokeResult = serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse smoke output as JSON: {stdout}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stderr.is_empty() {
            parsed.errors.push(stderr);
        }
    }
    Ok(parsed)
}

fn write_json_artifact(path: &str, value: &serde_json::Value) -> anyhow::Result<()> {
    let out_path = PathBuf::from(path);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
