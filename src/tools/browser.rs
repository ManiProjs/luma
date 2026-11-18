//! Bounded, one-shot Playwright automation for Teammates.
//!
//! The browser runs in a separate Node process. Screenshots are constrained to
//! the current task workspace and every invocation has a fixed time/output cap.

use std::{
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::Tool;

const MAX_STEPS: usize = 12;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 48 * 1024;
const MAX_TIMEOUT_MS: u64 = 60_000;

pub struct BrowserAutomate;

#[derive(Debug, Deserialize, Serialize)]
struct BrowserInput {
    steps: Vec<BrowserStep>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    allow_local: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum BrowserStep {
    Navigate { url: String },
    Snapshot,
    Click { selector: String },
    Fill { selector: String, value: String },
    Press { selector: String, key: String },
    Screenshot { path: String },
}

impl Tool for BrowserAutomate {
    fn name(&self) -> &str {
        "browser_automate"
    }

    fn description(&self) -> &str {
        "Automate a headless Playwright browser for this task in one bounded sequence. Input JSON: {\"steps\":[{\"action\":\"navigate\",\"url\":\"https://example.com\"},{\"action\":\"snapshot\"}],\"allow_local\":false}. Up to 12 steps. Screenshots must be inside the task workspace. This tool is mutating and requires approval. Set allow_local:true only when intentionally testing a localhost/loopback development server; private LAN and RFC1918 destinations remain blocked. Requires Node.js and Playwright Chromium. Set LUMA_NODE_BIN, LUMA_PLAYWRIGHT_MODULE, or LUMA_BROWSER_EXECUTABLE when the packaged runtime cannot locate them."
    }

    fn execute(&self, input: &str) -> Result<String> {
        if input.len() > MAX_INPUT_BYTES {
            bail!("browser input exceeds the {MAX_INPUT_BYTES} byte limit");
        }
        let mut input: BrowserInput = serde_json::from_str(input)
            .context("browser_automate expects a JSON object with steps")?;
        validate_input(&mut input)?;
        let workspace = std::env::current_dir().context("could not determine task workspace")?;
        let workspace = workspace
            .canonicalize()
            .context("could not resolve task workspace")?;
        for step in &mut input.steps {
            if let BrowserStep::Screenshot { path } = step {
                *path = screenshot_path(&workspace, path)?
                    .to_string_lossy()
                    .into_owned();
            }
        }
        run_playwright(input, &workspace)
    }

    fn requires_network(&self) -> bool {
        true
    }
}

fn validate_input(input: &mut BrowserInput) -> Result<()> {
    if input.steps.is_empty() || input.steps.len() > MAX_STEPS {
        bail!("browser steps must contain between 1 and {MAX_STEPS} actions");
    }
    let timeout = input.timeout_ms.unwrap_or(30_000);
    if !(1_000..=MAX_TIMEOUT_MS).contains(&timeout) {
        bail!("browser timeout_ms must be between 1000 and {MAX_TIMEOUT_MS}");
    }
    input.timeout_ms = Some(timeout);
    for step in &input.steps {
        match step {
            BrowserStep::Navigate { url } => {
                if url.len() > 4096 {
                    bail!("browser URL exceeds 4096 bytes");
                }
                validate_navigation_url(url, input.allow_local)?;
            }
            BrowserStep::Click { selector } => validate_selector(selector)?,
            BrowserStep::Fill { selector, value } => {
                validate_selector(selector)?;
                if value.len() > 10_000 {
                    bail!("browser fill value exceeds 10000 bytes");
                }
            }
            BrowserStep::Press { selector, key } => {
                validate_selector(selector)?;
                if key.is_empty() || key.len() > 64 || key.contains('\n') {
                    bail!("browser key must be a single key name of at most 64 bytes");
                }
            }
            BrowserStep::Screenshot { path } => {
                if path.len() > 512 {
                    bail!("screenshot path exceeds 512 bytes");
                }
                validate_relative_path(path)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_selector(selector: &str) -> Result<()> {
    if selector.trim().is_empty() || selector.len() > 512 {
        bail!("browser selector must be non-empty and at most 512 bytes");
    }
    Ok(())
}

fn validate_navigation_url(value: &str, allow_local: bool) -> Result<()> {
    let url = reqwest::Url::parse(value).context("browser navigate requires an absolute URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("browser navigation only supports HTTP(S) URLs without embedded credentials");
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let loopback_name = host == "localhost";
    if (host.ends_with(".localhost") && !loopback_name)
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        bail!("browser navigation to local hostnames is blocked");
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() && allow_local {
            return Ok(());
        }
        if !is_public_ip(ip) {
            bail!("browser navigation to private or local IP addresses is blocked");
        }
    } else if loopback_name && allow_local {
        return Ok(());
    } else if loopback_name {
        bail!("localhost navigation requires allow_local:true and tool approval");
    }
    Ok(())
}

fn is_public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_ip(std::net::IpAddr::V4(mapped));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.segments()[0] & 0xff00 == 0xff00)
        }
    }
}

fn validate_relative_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("screenshot path must be a relative path inside the task workspace");
    }
    Ok(())
}

fn screenshot_path(workspace: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    let workspace = workspace
        .canonicalize()
        .context("could not resolve task workspace")?;
    let path = workspace.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid screenshot path"))?;
    create_screenshot_parent(&workspace, parent)?;
    let parent = parent.canonicalize()?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !path.canonicalize()?.starts_with(&workspace) {
                bail!("screenshot destination escapes the task workspace");
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("could not inspect screenshot destination"),
    }
    Ok(parent.join(
        path.file_name()
            .ok_or_else(|| anyhow!("invalid screenshot filename"))?,
    ))
}

fn create_screenshot_parent(workspace: &Path, parent: &Path) -> Result<()> {
    let relative = parent
        .strip_prefix(workspace)
        .context("screenshot parent escapes the task workspace")?;
    let mut current = workspace.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("invalid screenshot directory component");
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!(
                        "screenshot directories cannot contain symlinks or non-directory entries"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current).with_context(|| {
                    format!(
                        "could not create screenshot directory {}",
                        current.display()
                    )
                })?;
                let metadata = std::fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("screenshot directory changed while being created");
                }
            }
            Err(error) => return Err(error).context("could not inspect screenshot directory"),
        }
        if !current.canonicalize()?.starts_with(workspace) {
            bail!("screenshot path escapes the task workspace");
        }
    }
    Ok(())
}

const NODE_SCRIPT: &str = r#"
const fs = require('node:fs');
const dns = require('node:dns').promises;
const net = require('node:net');
const http = require('node:http');
let chromium;
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const mappedV4 = ip => {
  if (net.isIP(ip) !== 6) return null;
  const halves = ip.toLowerCase().split('::');
  if (halves.length > 2) return null;
  const left = halves[0] ? halves[0].split(':') : [];
  const right = halves.length === 2 && halves[1] ? halves[1].split(':') : [];
  const groups = [...left, ...Array(Math.max(0, 8-left.length-right.length)).fill('0'), ...right];
  if (groups.length !== 8 || groups.slice(0,5).some(group => parseInt(group || '0',16) !== 0) || parseInt(groups[5],16) !== 0xffff) return null;
  const high = parseInt(groups[6],16), low = parseInt(groups[7],16);
  return [high >>> 8, high & 255, low >>> 8, low & 255].join('.');
};
const blocked = ip => {
  if (net.isIP(ip) === 4) {
    const p = ip.split('.').map(Number), a=p[0], b=p[1], c=p[2];
    return a===0 || a===10 || a===127 || a>=224 || (a===169&&b===254) || (a===172&&b>=16&&b<=31) || (a===192&&(b===168||(b===0&&(c===0||c===2)))) || (a===100&&b>=64&&b<=127) || (a===198&&(b===18||b===19||(b===51&&c===100))) || (a===203&&b===0&&c===113);
  }
  if (net.isIP(ip) === 6) {
    const mapped = mappedV4(ip);
    if (mapped) return blocked(mapped);
    return ip==='::1' || ip==='::' || /^f[cd]/i.test(ip) || /^fe[89ab]/i.test(ip) || /^ff/i.test(ip) || /^2001:(db8|10|20):/i.test(ip) || /^100:/i.test(ip);
  }
  return true;
};
const loopback = ip => {
  if (net.isIP(ip) === 4) return ip.startsWith('127.');
  if (net.isIP(ip) === 6) return ip === '::1' || !!mappedV4(ip)?.startsWith('127.');
  return false;
};
const localHostname = host => host === 'localhost' || host.endsWith('.localhost') || host.endsWith('.local') || host.endsWith('.internal');
async function resolveDestination(host) {
  host = host.toLowerCase();
  if (host.startsWith('[') && host.endsWith(']')) host = host.slice(1, -1);
  if (localHostname(host) && host !== 'localhost') throw new Error('local hostnames are blocked');
  if (host === 'localhost' && !input.allow_local) throw new Error('localhost requires allow_local:true');
  const addresses = net.isIP(host) ? [{address:host}] : await dns.lookup(host,{all:true,verbatim:true});
  if (!addresses.length) throw new Error('destination did not resolve');
  const allLoopback = addresses.every(item => loopback(item.address));
  if (input.allow_local && allLoopback) {
    return (addresses.find(item => net.isIP(item.address) === 4) || addresses[0]).address;
  }
  if (addresses.some(item => blocked(item.address))) throw new Error('destination resolves to a private or reserved address');
  return (addresses.find(item => net.isIP(item.address) === 4) || addresses[0]).address;
}
function rejectResponse(response, status, message) {
  response.writeHead(status, {'content-type':'text/plain','connection':'close'});
  response.end(message);
}
const proxy = http.createServer((request,response) => {
  void (async () => {
    const target = new URL(request.url);
    if (target.protocol !== 'http:' || target.username || target.password) throw new Error('only plain HTTP proxy requests are accepted');
    const address = await resolveDestination(target.hostname);
    const port = Number(target.port || 80);
    if (!(input.allow_local && loopback(address)) && port !== 80) throw new Error('public HTTP connections are limited to port 80');
    const upstream = http.request({
      host:address, port, method:request.method, path:target.pathname + target.search,
      headers:{...request.headers,host:target.host}, agent:false
    }, result => {
      response.writeHead(result.statusCode || 502,result.headers);
      result.pipe(response);
    });
    upstream.on('error',() => rejectResponse(response,502,'upstream request failed'));
    request.pipe(upstream);
  })().catch(error => rejectResponse(response,403,String(error.message || error).slice(0,300)));
});
const proxySockets = new Set();
proxy.on('connection',socket => {
  proxySockets.add(socket);
  socket.once('close',() => proxySockets.delete(socket));
});
proxy.on('connect',(request,client,head) => {
  void (async () => {
    const target = new URL('http://' + request.url);
    const address = await resolveDestination(target.hostname);
    const port = Number(target.port || 443);
    if (!(input.allow_local && loopback(address)) && port !== 443) throw new Error('public HTTPS connections are limited to port 443');
    const upstream = net.connect({host:address,port});
    upstream.once('connect',() => {
      client.write('HTTP/1.1 200 Connection Established' + String.fromCharCode(13,10,13,10));
      if (head.length) upstream.write(head);
      upstream.pipe(client); client.pipe(upstream);
    });
    upstream.once('error',() => client.end('HTTP/1.1 502 Bad Gateway' + String.fromCharCode(13,10,13,10)));
  })().catch(() => client.end('HTTP/1.1 403 Forbidden' + String.fromCharCode(13,10,13,10)));
});
proxy.on('upgrade',(request,client,head) => {
  void (async () => {
    const target = new URL(request.url);
    if (target.protocol !== 'ws:' || target.username || target.password) throw new Error('unsupported WebSocket destination');
    const address = await resolveDestination(target.hostname);
    const port = Number(target.port || 80);
    if (!(input.allow_local && loopback(address)) && port !== 80) throw new Error('public WebSocket connections are limited to port 80');
    const upstream = net.connect({host:address,port});
    upstream.once('connect',() => {
      const crlf = String.fromCharCode(13,10);
      const headers = Object.entries({...request.headers,host:target.host}).map(([key,value]) => `${key}: ${value}`).join(crlf);
      upstream.write(`${request.method} ${target.pathname}${target.search} HTTP/${request.httpVersion}${crlf}${headers}${crlf}${crlf}`);
      if (head.length) upstream.write(head);
      upstream.pipe(client); client.pipe(upstream);
    });
    upstream.once('error',() => client.destroy());
  })().catch(() => client.end('HTTP/1.1 403 Forbidden' + String.fromCharCode(13,10,13,10)));
});
(async () => {
  let browser;
  try {
    ({chromium} = require(process.env.LUMA_PLAYWRIGHT_MODULE || 'playwright'));
    await new Promise((resolve,reject) => {
      proxy.once('error',reject);
      proxy.listen(0,'127.0.0.1',resolve);
    });
    const proxyAddress = proxy.address();
    browser = await chromium.launch({
      headless:true,
      proxy:{server:`http://127.0.0.1:${proxyAddress.port}`},
      args:['--proxy-bypass-list=<-loopback>','--disable-quic','--disable-features=AsyncDns','--force-webrtc-ip-handling-policy=disable_non_proxied_udp'],
      ...(process.env.LUMA_BROWSER_EXECUTABLE ? {executablePath:process.env.LUMA_BROWSER_EXECUTABLE} : {})
    });
    const context = await browser.newContext({acceptDownloads:false,serviceWorkers:'block'});
    const page = await context.newPage();
    await page.route('**/*', async route => {
      try {
        const u = new URL(route.request().url());
        if (!['http:','https:','ws:','wss:','data:','blob:','about:'].includes(u.protocol)) return route.abort();
        return route.continue();
      } catch { return route.abort(); }
    });
    const results=[];
    for (const step of input.steps) {
      if (step.action==='navigate') {
        const response=await page.goto(step.url,{waitUntil:'domcontentloaded',timeout:input.timeout_ms});
        results.push({action:'navigate',url:page.url(),status:response?.status() ?? null,title:await page.title()});
      } else if (step.action==='snapshot') {
        results.push({action:'snapshot',url:page.url(),title:await page.title(),text:(await page.locator('body').innerText({timeout:input.timeout_ms})).slice(0,2000)});
      } else if (step.action==='click') {
        await page.locator(step.selector).click({timeout:input.timeout_ms}); results.push({action:'click',selector:step.selector,ok:true});
      } else if (step.action==='fill') {
        await page.locator(step.selector).fill(step.value,{timeout:input.timeout_ms}); results.push({action:'fill',selector:step.selector,ok:true});
      } else if (step.action==='press') {
        await page.locator(step.selector).press(step.key,{timeout:input.timeout_ms}); results.push({action:'press',selector:step.selector,key:step.key,ok:true});
      } else if (step.action==='screenshot') {
        await page.screenshot({path:step.path,fullPage:true,timeout:input.timeout_ms}); results.push({action:'screenshot',path:step.path});
      }
    }
    process.stdout.write(JSON.stringify({ok:true,results}));
  } catch (error) {
    const message = String(error && error.message || error);
    process.stdout.write(JSON.stringify({ok:false,error:message.includes('Executable doesn\'t exist') ? 'Playwright Chromium is not installed. Install it with `npx playwright install chromium`, or configure the packaged browser executable.' : message.slice(0,2000)}));
    process.exitCode=1;
  } finally {
    if (browser) await browser.close().catch(()=>{});
    for (const socket of proxySockets) socket.destroy();
    if (proxy.listening) await new Promise(resolve => proxy.close(resolve));
  }
})();
"#;

fn run_playwright(input: BrowserInput, workspace: &Path) -> Result<String> {
    let node = std::env::var_os("LUMA_NODE_BIN").unwrap_or_else(|| "node".into());
    let mut command = Command::new(node);
    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "USER",
        "LOGNAME",
        "SystemRoot",
        "WINDIR",
        "APPDATA",
        "LOCALAPPDATA",
        "PLAYWRIGHT_BROWSERS_PATH",
        "NODE_EXTRA_CA_CERTS",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    if let Some(browser) = std::env::var_os("LUMA_BROWSER_EXECUTABLE") {
        command.env("LUMA_BROWSER_EXECUTABLE", browser);
    }
    command
        .arg("-e")
        .arg(NODE_SCRIPT)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(module) = std::env::var_os("LUMA_PLAYWRIGHT_MODULE") {
        command.env("LUMA_PLAYWRIGHT_MODULE", module);
    } else {
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("desktop/node_modules/playwright");
        if bundled.exists() {
            command.env("LUMA_PLAYWRIGHT_MODULE", bundled);
        }
    }
    let mut child = command
        .spawn()
        .context("could not start Node.js; install Node.js or set LUMA_NODE_BIN")?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("could not open browser process stdin"))?;
        serde_json::to_writer(&mut *stdin, &input).context("could not send browser steps")?;
        stdin.flush()?;
    }
    drop(child.stdin.take());
    let timeout = Duration::from_millis(input.timeout_ms.unwrap_or(30_000).saturating_add(5_000));
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            let output = child
                .wait_with_output()
                .context("could not collect browser output")?;
            if output.stdout.len() > MAX_OUTPUT_BYTES {
                bail!("browser process exceeded its output limit");
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() {
                let value: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
                let message = value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| "Playwright browser automation failed".into());
                bail!("{message}");
            }
            return Ok(stdout.into_owned());
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "browser automation exceeded its {} ms time limit",
                timeout.as_millis()
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounded_step_sequences_and_selectors() {
        let mut input: BrowserInput =
            serde_json::from_str(r#"{"steps":[{"action":"click","selector":"button.save"}]}"#)
                .unwrap();
        assert!(validate_input(&mut input).is_ok());
        assert_eq!(input.timeout_ms, Some(30_000));
        assert!(validate_selector("").is_err());
        assert!(validate_selector(&"x".repeat(513)).is_err());
        let mut too_many: BrowserInput = serde_json::from_value(serde_json::json!({"steps":(0..13).map(|_| serde_json::json!({"action":"snapshot"})).collect::<Vec<_>>()})).unwrap();
        assert!(validate_input(&mut too_many).is_err());
    }

    #[test]
    fn browser_uses_a_pinned_ip_proxy_for_every_network_connection() {
        assert!(NODE_SCRIPT.contains("dns.lookup(host,{all:true,verbatim:true})"));
        assert!(NODE_SCRIPT.contains("net.connect({host:address,port})"));
        assert!(NODE_SCRIPT.contains("--proxy-bypass-list=<-loopback>"));
        assert!(!NODE_SCRIPT.contains("dns.lookup(hostname"));
    }

    #[test]
    fn explicit_loopback_browser_integration() {
        if std::env::var_os("LUMA_RUN_BROWSER_INTEGRATION").is_none() {
            return;
        }
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            let body = "<!doctype html><title>Luma browser test</title><p>pinned proxy works</p>";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let input = serde_json::json!({
            "allow_local": true,
            "timeout_ms": 20_000,
            "steps": [
                {"action":"navigate","url":format!("http://{address}/")},
                {"action":"snapshot"}
            ]
        });
        let output = BrowserAutomate.execute(&input.to_string()).unwrap();
        server.join().unwrap();
        assert!(output.contains("pinned proxy works"), "{output}");
    }

    #[test]
    fn local_urls_require_explicit_approval_and_private_lan_stays_blocked() {
        assert!(validate_navigation_url("https://example.com", false).is_ok());
        assert!(validate_navigation_url("http://localhost:3000", false).is_err());
        assert!(validate_navigation_url("http://localhost:3000", true).is_ok());
        assert!(validate_navigation_url("http://127.0.0.1:3000", false).is_err());
        assert!(validate_navigation_url("http://127.0.0.1:3000", true).is_ok());
        assert!(validate_navigation_url("http://[::1]:3000", true).is_ok());
        for url in [
            "file:///etc/passwd",
            "http://192.168.1.2",
            "http://10.0.0.2",
            "http://172.16.0.1",
            "http://192.168.1.2",
            "https://user:pass@example.com",
            "http://dev.localhost",
        ] {
            assert!(validate_navigation_url(url, true).is_err(), "{url}");
        }
    }

    #[test]
    fn screenshot_paths_are_relative_and_cannot_escape_workspace() {
        assert!(validate_relative_path("screenshots/page.png").is_ok());
        for path in ["../outside.png", "/tmp/outside.png", "a/../outside.png", ""] {
            assert!(validate_relative_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn screenshot_paths_reject_symlinked_workspace_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("assets/linked")).unwrap();
        #[cfg(unix)]
        assert!(screenshot_path(root.path(), "assets/linked/shot.png").is_err());
        assert!(screenshot_path(root.path(), "assets/shot.png").is_ok());
    }

    #[test]
    fn screenshot_paths_create_missing_directories_without_following_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = root.path().canonicalize().unwrap();
        let destination = screenshot_path(root.path(), "artifacts/nested/page.png").unwrap();
        assert!(destination.starts_with(&canonical_root));
        assert!(root.path().join("artifacts/nested").is_dir());
        let outside = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
            assert!(screenshot_path(root.path(), "linked/page.png").is_err());
        }
    }
}
