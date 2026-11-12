//! Network tools reserved for persistent personal agents (Teammates).

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use reqwest::{Client, Method, Url, header};
use serde::Deserialize;

use super::Tool;

const MAX_RESPONSE_BYTES: usize = 1_000_000;
const MAX_TEXT_BYTES: usize = 24_000;

pub struct WebSearch;
pub struct FetchUrl;
pub struct HttpRequest;

impl Tool for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the public web. Input: a search query, or JSON {\"query\":\"...\"}. Returns up to 8 results with titles, URLs and snippets."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let query = if let Ok(value) = serde_json::from_str::<serde_json::Value>(input) {
            value
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned()
        } else {
            input.trim().to_owned()
        };
        if query.is_empty() {
            bail!("web_search requires a non-empty query");
        }
        run_async(async move { search_web(&query).await })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for FetchUrl {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Fetch a public HTTP(S) webpage and return readable text. Input: a URL, or JSON {\"url\":\"https://...\"}. Does not execute page scripts."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let url = if let Ok(value) = serde_json::from_str::<serde_json::Value>(input) {
            value
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_owned()
        } else {
            input.trim().to_owned()
        };
        if url.is_empty() {
            bail!("fetch_url requires a URL");
        }
        run_async(async move { fetch_url(&url).await })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn requires_network(&self) -> bool {
        true
    }
}

#[derive(Deserialize)]
struct HttpRequestInput {
    method: String,
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<String>,
}

impl Tool for HttpRequest {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Send an HTTP request to a public URL. Input JSON: {\"method\":\"GET|POST|PUT|PATCH|DELETE|HEAD\",\"url\":\"https://...\",\"headers\":{},\"body\":\"optional text\"}. Every request requires approval."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let request: HttpRequestInput = serde_json::from_str(input)
            .context("http_request expects a JSON object with method and url")?;
        run_async(async move { send_http_request(request).await })
    }

    fn requires_network(&self) -> bool {
        true
    }
}

fn run_async<F>(future: F) -> Result<String>
where
    F: std::future::Future<Output = Result<String>> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => Err(anyhow!(error)),
        };
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| anyhow!("network tool worker thread panicked"))?
}

fn client(host: &str, addresses: &[SocketAddr]) -> Result<Client> {
    Ok(Client::builder()
        .user_agent("Luma/0.1 (+https://github.com/ManiProjs/luma)")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        // Pin the already-validated DNS answers so a hostname cannot resolve
        // to a private address between validation and connection.
        .resolve_to_addrs(host, addresses)
        .build()?)
}

async fn validate_public_url(url: &Url) -> Result<Vec<SocketAddr>> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("only public HTTP and HTTPS URLs are supported");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("URL has no hostname"))?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        bail!("requests to local network hostnames are blocked");
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("URL has no valid port"))?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("could not resolve hostname {host}"))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        bail!("requests to private, local, or non-public IP addresses are blocked");
    }
    Ok(addresses)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            let reserved = (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && (octets[2] == 0 || octets[2] == 2))
                || (octets[0] == 198
                    && ((octets[1] == 18 || octets[1] == 19)
                        || (octets[1] == 51 && octets[2] == 100)))
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113);
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || octets[0] == 0
                || octets[0] >= 224
                || reserved)
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(mapped));
            }
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.segments()[0] & 0xff00 == 0xff00)
        }
    }
}

async fn send_with_safe_redirects(
    method: Method,
    mut url: Url,
    body: Option<String>,
    headers: BTreeMap<String, String>,
) -> Result<reqwest::Response> {
    let original_host = url.host_str().unwrap_or_default().to_owned();
    for _ in 0..=4 {
        let addresses = validate_public_url(&url).await?;
        let host = url.host_str().unwrap_or_default();
        let client = client(host, &addresses)?;
        let mut request = client.request(method.clone(), url.clone());
        for (name, value) in &headers {
            request = request.header(name, value);
        }
        if let Some(body) = &body {
            request = request.body(body.clone());
        }
        let response = request.send().await.context("HTTP request failed")?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| anyhow!("redirect did not include a valid Location header"))?;
        let next = url.join(location).context("invalid redirect URL")?;
        if next.host_str().unwrap_or_default() != original_host
            && (!headers.is_empty() || body.is_some())
        {
            bail!("cross-host redirects are blocked to prevent credential leakage");
        }
        url = next;
    }
    bail!("too many redirects")
}

async fn search_web(query: &str) -> Result<String> {
    let mut url = Url::parse("https://html.duckduckgo.com/html/")?;
    url.query_pairs_mut().append_pair("q", query);
    let response = send_with_safe_redirects(Method::GET, url, None, BTreeMap::new()).await?;
    let status = response.status();
    let html =
        String::from_utf8_lossy(&read_limited(response, MAX_RESPONSE_BYTES).await?).into_owned();
    if !status.is_success() {
        bail!("web search returned HTTP {status}");
    }
    let results = parse_search_results(&html);
    if results.is_empty() {
        Ok(format!(
            "No results were parsed for `{query}`. Try a more specific query or fetch a known URL."
        ))
    } else {
        Ok(format!(
            "Web results for: {query}\n\n{}",
            results.join("\n\n")
        ))
    }
}

fn parse_search_results(html: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut rest = html;
    while results.len() < 8 {
        let Some(marker) = rest.find("class=\"result__a\"") else {
            break;
        };
        let before = &rest[..marker];
        let Some(anchor_start) = before.rfind("<a ") else {
            rest = &rest[marker + 1..];
            continue;
        };
        let Some(tag_end) = rest[marker..].find('>') else {
            break;
        };
        let opening = format!(
            "{}{}",
            &before[anchor_start..],
            &rest[marker..marker + tag_end]
        );
        let href = opening
            .split("href=\"")
            .nth(1)
            .and_then(|value| value.split('\"').next())
            .unwrap_or("");
        let Some(end) = rest[marker + tag_end + 1..].find("</a>") else {
            break;
        };
        let title = clean_html(&rest[marker + tag_end + 1..marker + tag_end + 1 + end]);
        let next_offset = marker + tag_end + 1 + end + 4;
        let remaining = &rest[next_offset..];
        let snippet = remaining
            .find("result__snippet")
            .and_then(|index| {
                let after = &remaining[index..];
                let start = after.find('>')? + 1;
                let stop = after[start..].find('<')?;
                Some(clean_html(&after[start..start + stop]))
            })
            .unwrap_or_default();
        let href = decode_entities(href);
        let target = if href.starts_with("//") {
            format!("https:{href}")
        } else {
            href
        };
        if !title.is_empty() && (target.starts_with("https://") || target.starts_with("http://")) {
            results.push(format!(
                "{}. {}\n   {}\n   {}",
                results.len() + 1,
                title,
                target,
                snippet
            ));
        }
        rest = &rest[next_offset..];
    }
    results
}

async fn fetch_url(input: &str) -> Result<String> {
    let url = Url::parse(input).context("fetch_url requires an absolute URL")?;
    let response = send_with_safe_redirects(Method::GET, url, None, BTreeMap::new()).await?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();
    let bytes = read_limited(response, MAX_RESPONSE_BYTES).await?;
    if !status.is_success() {
        bail!("fetch_url returned HTTP {status}");
    }
    let body = String::from_utf8_lossy(&bytes);
    let text = if content_type.contains("html") {
        clean_html(&body)
    } else {
        body.into_owned()
    };
    Ok(format!(
        "URL: {input}\nHTTP: {status}\nContent-Type: {content_type}\n\n{}",
        truncate(&text, MAX_TEXT_BYTES)
    ))
}

async fn send_http_request(input: HttpRequestInput) -> Result<String> {
    let method = Method::from_bytes(input.method.trim().to_ascii_uppercase().as_bytes())
        .context("unsupported HTTP method")?;
    if !matches!(
        method,
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        bail!("supported methods are GET, HEAD, POST, PUT, PATCH and DELETE");
    }
    let url = Url::parse(&input.url).context("http_request requires an absolute URL")?;
    let response = send_with_safe_redirects(method, url, input.body, input.headers).await?;
    let status = response.status();
    let body =
        String::from_utf8_lossy(&read_limited(response, MAX_RESPONSE_BYTES).await?).into_owned();
    Ok(format!(
        "HTTP {status}\n\n{}",
        truncate(&body, MAX_TEXT_BYTES)
    ))
}

async fn read_limited(response: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("response exceeded the {} byte limit", max_bytes);
    }
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("could not read HTTP response body")?;
        if output.len().saturating_add(chunk.len()) > max_bytes {
            bail!("response exceeded the {} byte limit", max_bytes);
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn clean_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len().min(MAX_TEXT_BYTES));
    let mut in_tag = false;
    let mut skip_script = false;
    let mut tag = String::new();
    for character in input.chars() {
        if character == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        if in_tag {
            if character == '>' {
                let normalized = tag.trim().to_ascii_lowercase();
                if normalized.starts_with("script") || normalized.starts_with("style") {
                    skip_script = true;
                }
                if normalized.starts_with("/script") || normalized.starts_with("/style") {
                    skip_script = false;
                }
                if normalized.starts_with("br")
                    || normalized.starts_with("/p")
                    || normalized.starts_with("/div")
                    || normalized.starts_with("/li")
                    || normalized.starts_with("/h")
                {
                    output.push('\n');
                }
                in_tag = false;
            } else if tag.len() < 32 {
                tag.push(character);
            }
            continue;
        }
        if !skip_script {
            output.push(character);
        }
    }
    decode_entities(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_owned();
    }
    let mut end = max;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… response truncated", &input[..end])
}

#[cfg(test)]
mod tests {
    use super::{clean_html, is_public_ip, parse_search_results};
    use std::net::IpAddr;

    #[test]
    fn strips_html_and_decodes_entities() {
        assert_eq!(clean_html("<p>Hello &amp; goodbye</p>"), "Hello & goodbye");
    }

    #[test]
    fn rejects_private_and_loopback_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "::1",
            "fc00::1",
        ] {
            assert!(
                !is_public_ip(address.parse::<IpAddr>().unwrap()),
                "{address}"
            );
        }
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn parses_search_links_when_href_follows_class_attribute() {
        let html = r#"<div class="result"><a rel="nofollow" class="result__a" href="https://example.com/?a=1&amp;b=2">Example &amp; test</a><a class="result__snippet">A useful snippet</a></div>"#;
        let results = parse_search_results(html);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("Example & test"));
        assert!(results[0].contains("https://example.com/?a=1&b=2"));
        assert!(results[0].contains("A useful snippet"));
    }

    #[test]
    fn network_tools_are_exposed_only_to_teammates() {
        let code_tools = super::super::default_registry().names();
        let teammate_tools = super::super::teammate_registry().names();
        for name in ["web_search", "fetch_url", "http_request"] {
            assert!(!code_tools.iter().any(|tool| tool == name));
            assert!(teammate_tools.iter().any(|tool| tool == name));
        }
    }
}
