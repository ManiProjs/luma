//! First-party GitHub REST tools for Teammates.
//!
//! All requests are pinned to api.github.com. Authentication is read from
//! GH_TOKEN or GITHUB_TOKEN at execution time and is never included in output.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, Method, StatusCode, Url, header};
use serde::Deserialize;
use serde_json::{Value, json};

use super::Tool;

const API_BASE: &str = "https://api.github.com";
const MAX_RESPONSE_BYTES: usize = 1_000_000;
const MAX_OUTPUT_BYTES: usize = 24_000;
const MAX_INPUT_BYTES: usize = 64_000;
const MAX_PER_PAGE: u32 = 100;

pub struct GetRepository;
pub struct ListIssues;
pub struct ListPullRequests;
pub struct GetPullRequest;
pub struct GetPullRequestFiles;
pub struct ListPullRequestReviews;
pub struct ListWorkflowRuns;
pub struct CreateIssue;
pub struct CommentOnIssue;
pub struct CreatePullRequest;

#[derive(Debug, Deserialize)]
struct RepoInput {
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
struct ListInput {
    owner: String,
    repo: String,
    #[serde(default = "default_per_page")]
    per_page: u32,
    #[serde(default = "default_page")]
    page: u32,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NumberInput {
    owner: String,
    repo: String,
    number: u64,
}

#[derive(Debug, Deserialize)]
struct PullRequestListInput {
    owner: String,
    repo: String,
    number: u64,
    #[serde(default = "default_per_page")]
    per_page: u32,
    #[serde(default = "default_page")]
    page: u32,
}

#[derive(Debug, Deserialize)]
struct CreateIssueInput {
    owner: String,
    repo: String,
    title: String,
    body: Option<String>,
    labels: Option<Vec<String>>,
    assignees: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CommentInput {
    owner: String,
    repo: String,
    issue_number: u64,
    body: String,
}

#[derive(Debug, Deserialize)]
struct CreatePullRequestInput {
    owner: String,
    repo: String,
    title: String,
    head: String,
    base: String,
    body: Option<String>,
    draft: Option<bool>,
}

impl Tool for GetRepository {
    fn name(&self) -> &str {
        "github_get_repository"
    }
    fn description(&self) -> &str {
        "Read a GitHub repository. Input JSON: {\"owner\":\"...\",\"repo\":\"...\"}. Returns repository metadata."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: RepoInput = parse_input(input)?;
        run_async(async move {
            request(
                Method::GET,
                &format!(
                    "/repos/{}/{}",
                    segment(&input.owner)?,
                    segment(&input.repo)?
                ),
                None,
                false,
            )
            .await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for ListIssues {
    fn name(&self) -> &str {
        "github_list_issues"
    }
    fn description(&self) -> &str {
        "List GitHub issues, excluding pull requests. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"state\":\"open|closed|all\",\"page\":1,\"per_page\":30}. Pages are bounded to 100 items."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: ListInput = parse_input(input)?;
        validate_page(input.page, input.per_page)?;
        let state = validate_state(input.state.as_deref())?;
        run_async(async move {
            let mut path = format!(
                "/repos/{}/{}/issues",
                segment(&input.owner)?,
                segment(&input.repo)?
            );
            append_query(&mut path, input.page, input.per_page, state.as_deref());
            request(Method::GET, &path, None, true).await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for ListPullRequests {
    fn name(&self) -> &str {
        "github_list_pull_requests"
    }
    fn description(&self) -> &str {
        "List GitHub pull requests. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"state\":\"open|closed|all\",\"page\":1,\"per_page\":30}. Pages are bounded to 100 items."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: ListInput = parse_input(input)?;
        validate_page(input.page, input.per_page)?;
        let state = validate_state(input.state.as_deref())?;
        run_async(async move {
            let mut path = format!(
                "/repos/{}/{}/pulls",
                segment(&input.owner)?,
                segment(&input.repo)?
            );
            append_query(&mut path, input.page, input.per_page, state.as_deref());
            request(Method::GET, &path, None, false).await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for GetPullRequest {
    fn name(&self) -> &str {
        "github_get_pull_request"
    }
    fn description(&self) -> &str {
        "Read a GitHub pull request. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"number\":123}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: NumberInput = parse_input(input)?;
        validate_number(input.number)?;
        run_async(async move {
            request(
                Method::GET,
                &format!(
                    "/repos/{}/{}/pulls/{}",
                    segment(&input.owner)?,
                    segment(&input.repo)?,
                    input.number
                ),
                None,
                false,
            )
            .await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for GetPullRequestFiles {
    fn name(&self) -> &str {
        "github_get_pull_request_files"
    }
    fn description(&self) -> &str {
        "List files changed by a GitHub pull request, including patch text when GitHub provides it. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"number\":123,\"page\":1,\"per_page\":30}. Pages are bounded to 100 items."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: PullRequestListInput = parse_input(input)?;
        validate_number(input.number)?;
        validate_page(input.page, input.per_page)?;
        run_async(async move {
            let path = pull_request_files_path(
                &input.owner,
                &input.repo,
                input.number,
                input.page,
                input.per_page,
            )?;
            request(Method::GET, &path, None, false).await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for ListPullRequestReviews {
    fn name(&self) -> &str {
        "github_list_pull_request_reviews"
    }
    fn description(&self) -> &str {
        "List reviews and review states for a GitHub pull request. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"number\":123,\"page\":1,\"per_page\":30}. Pages are bounded to 100 items."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: PullRequestListInput = parse_input(input)?;
        validate_number(input.number)?;
        validate_page(input.page, input.per_page)?;
        run_async(async move {
            let path = pull_request_reviews_path(
                &input.owner,
                &input.repo,
                input.number,
                input.page,
                input.per_page,
            )?;
            request(Method::GET, &path, None, false).await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

fn pull_request_files_path(
    owner: &str,
    repo: &str,
    number: u64,
    page: u32,
    per_page: u32,
) -> Result<String> {
    validate_number(number)?;
    validate_page(page, per_page)?;
    let mut path = format!(
        "/repos/{}/{}/pulls/{number}/files",
        segment(owner)?,
        segment(repo)?
    );
    append_query(&mut path, page, per_page, None);
    Ok(path)
}

fn pull_request_reviews_path(
    owner: &str,
    repo: &str,
    number: u64,
    page: u32,
    per_page: u32,
) -> Result<String> {
    validate_number(number)?;
    validate_page(page, per_page)?;
    let mut path = format!(
        "/repos/{}/{}/pulls/{number}/reviews",
        segment(owner)?,
        segment(repo)?
    );
    append_query(&mut path, page, per_page, None);
    Ok(path)
}

impl Tool for ListWorkflowRuns {
    fn name(&self) -> &str {
        "github_list_workflow_runs"
    }
    fn description(&self) -> &str {
        "List GitHub Actions workflow runs. Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"page\":1,\"per_page\":30}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: ListInput = parse_input(input)?;
        validate_page(input.page, input.per_page)?;
        run_async(async move {
            let mut path = format!(
                "/repos/{}/{}/actions/runs",
                segment(&input.owner)?,
                segment(&input.repo)?
            );
            append_query(&mut path, input.page, input.per_page, None);
            request(Method::GET, &path, None, false).await
        })
    }
    fn is_read_only(&self) -> bool {
        true
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for CreateIssue {
    fn name(&self) -> &str {
        "github_create_issue"
    }
    fn description(&self) -> &str {
        "Create a GitHub issue (requires approval). Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"title\":\"...\",\"body\":\"optional\",\"labels\":[...],\"assignees\":[...]}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: CreateIssueInput = parse_input(input)?;
        validate_text(&input.title, "title")?;
        let body = json!({"title": input.title, "body": input.body, "labels": input.labels, "assignees": input.assignees});
        run_async(async move {
            request(
                Method::POST,
                &format!(
                    "/repos/{}/{}/issues",
                    segment(&input.owner)?,
                    segment(&input.repo)?
                ),
                Some(body),
                false,
            )
            .await
        })
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for CommentOnIssue {
    fn name(&self) -> &str {
        "github_comment_on_issue"
    }
    fn description(&self) -> &str {
        "Add a comment to a GitHub issue or pull request (requires approval). Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"issue_number\":123,\"body\":\"...\"}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: CommentInput = parse_input(input)?;
        validate_number(input.issue_number)?;
        validate_text(&input.body, "body")?;
        run_async(async move {
            request(
                Method::POST,
                &format!(
                    "/repos/{}/{}/issues/{}/comments",
                    segment(&input.owner)?,
                    segment(&input.repo)?,
                    input.issue_number
                ),
                Some(json!({"body": input.body})),
                false,
            )
            .await
        })
    }
    fn requires_network(&self) -> bool {
        true
    }
}

impl Tool for CreatePullRequest {
    fn name(&self) -> &str {
        "github_create_pull_request"
    }
    fn description(&self) -> &str {
        "Create a GitHub pull request (requires approval). Input JSON: {\"owner\":\"...\",\"repo\":\"...\",\"title\":\"...\",\"head\":\"branch\",\"base\":\"main\",\"body\":\"optional\",\"draft\":false}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let input: CreatePullRequestInput = parse_input(input)?;
        validate_text(&input.title, "title")?;
        validate_text(&input.head, "head")?;
        validate_text(&input.base, "base")?;
        let body = json!({"title": input.title, "head": input.head, "base": input.base, "body": input.body, "draft": input.draft.unwrap_or(false)});
        run_async(async move {
            request(
                Method::POST,
                &format!(
                    "/repos/{}/{}/pulls",
                    segment(&input.owner)?,
                    segment(&input.repo)?
                ),
                Some(body),
                false,
            )
            .await
        })
    }
    fn requires_network(&self) -> bool {
        true
    }
}

fn parse_input<T: for<'de> Deserialize<'de>>(input: &str) -> Result<T> {
    if input.len() > MAX_INPUT_BYTES {
        bail!("GitHub tool input exceeds the {MAX_INPUT_BYTES} byte limit");
    }
    serde_json::from_str(input).context("GitHub tool expects a JSON object")
}

fn default_per_page() -> u32 {
    30
}
fn default_page() -> u32 {
    1
}

fn validate_page(page: u32, per_page: u32) -> Result<()> {
    if page == 0 || per_page == 0 || per_page > MAX_PER_PAGE {
        bail!("page must be at least 1 and per_page must be between 1 and {MAX_PER_PAGE}");
    }
    Ok(())
}

fn validate_state(state: Option<&str>) -> Result<Option<String>> {
    match state {
        None => Ok(None),
        Some("open" | "closed" | "all") => Ok(state.map(str::to_owned)),
        Some(_) => bail!("state must be open, closed, or all"),
    }
}

fn append_query(path: &mut String, page: u32, per_page: u32, state: Option<&str>) {
    path.push_str(&format!("?page={page}&per_page={per_page}"));
    if let Some(state) = state {
        path.push_str("&state=");
        path.push_str(state);
    }
}

fn segment(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        || value == "."
        || value == ".."
    {
        bail!("repository owner and name must be valid GitHub path segments");
    }
    Ok(value.to_owned())
}

fn validate_text(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 65_536 {
        bail!("{field} must be non-empty and at most 65536 bytes");
    }
    Ok(())
}

fn validate_number(number: u64) -> Result<()> {
    if number == 0 {
        bail!("GitHub issue and pull request numbers must be greater than zero");
    }
    Ok(())
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
        .map_err(|_| anyhow!("GitHub request worker thread panicked"))?
}

fn api_client(token: Option<&str>) -> Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        header::HeaderValue::from_static("2022-11-28"),
    );
    if let Some(token) = token {
        let mut value = header::HeaderValue::from_str(&format!("Bearer {token}"))
            .context("GitHub token contains invalid header characters")?;
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
    }
    Client::builder()
        .default_headers(headers)
        .user_agent("Luma/0.1 (+https://github.com/ManiProjs/luma)")
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("could not build GitHub API client")
}

async fn request(
    method: Method,
    path: &str,
    body: Option<Value>,
    exclude_pull_requests: bool,
) -> Result<String> {
    let url = api_url(path)?;
    let token = std::env::var("GH_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("GITHUB_TOKEN")
                .ok()
                .filter(|v| !v.trim().is_empty())
        });
    let client = api_client(token.as_deref())?;
    let mut request = client.request(method, url);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.context("GitHub API request failed")?;
    let status = response.status();
    let next_page = response
        .headers()
        .get(header::LINK)
        .and_then(|value| value.to_str().ok())
        .is_some_and(link_has_next);
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = read_limited(response, MAX_RESPONSE_BYTES).await?;
    let mut value: Value = if bytes.is_empty() {
        json!({"status":"success"})
    } else {
        serde_json::from_slice(&bytes).context("GitHub returned invalid JSON")?
    };
    if exclude_pull_requests {
        value = without_pull_requests(value);
    }
    if !status.is_success() {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("GitHub API request failed");
        if is_rate_limit(status, remaining.as_deref(), retry_after.is_some()) {
            bail!(
                "GitHub rate limit reached: {}{}{}",
                redact(message, token.as_deref()),
                retry_after
                    .map(|v| format!("; retry after {v}s"))
                    .unwrap_or_default(),
                reset.map(|v| format!("; reset at {v}")).unwrap_or_default()
            );
        }
        bail!(
            "GitHub API returned HTTP {}: {}",
            status.as_u16(),
            safe_message(&redact(message, token.as_deref()))
        );
    }
    let mut output = format_output(&redact_value(value, token.as_deref()))?;
    if next_page {
        output.push_str("\n\nPagination: another page is available; increase `page` to continue.");
    }
    Ok(truncate(&output, MAX_OUTPUT_BYTES))
}

fn without_pull_requests(mut value: Value) -> Value {
    if let Value::Array(items) = &mut value {
        items.retain(|item| item.get("pull_request").is_none());
    }
    value
}

fn api_url(path: &str) -> Result<Url> {
    if !path.starts_with("/repos/") || path.contains("..") || path.starts_with("//") {
        bail!("invalid GitHub API path");
    }
    let url = Url::parse(&format!("{API_BASE}{path}"))?;
    if url.host_str() != Some("api.github.com") || url.scheme() != "https" {
        bail!("GitHub API requests must use api.github.com over HTTPS");
    }
    Ok(url)
}

fn safe_message(message: &str) -> String {
    truncate(message, 1000)
}

fn is_rate_limit(status: StatusCode, remaining: Option<&str>, has_retry_after: bool) -> bool {
    (status == StatusCode::FORBIDDEN && remaining == Some("0"))
        || status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && has_retry_after)
}

fn link_has_next(header: &str) -> bool {
    header.split(',').any(|part| {
        part.split(';')
            .any(|parameter| parameter.trim() == "rel=\"next\"")
    })
}

fn redact(value: &str, token: Option<&str>) -> String {
    token
        .filter(|token| !token.is_empty())
        .map(|token| value.replace(token, "[REDACTED]"))
        .unwrap_or_else(|| value.to_owned())
}

fn redact_value(mut value: Value, token: Option<&str>) -> Value {
    fn visit(value: &mut Value, token: Option<&str>) {
        match value {
            Value::String(text) => *text = redact(text, token),
            Value::Array(items) => items.iter_mut().for_each(|item| visit(item, token)),
            Value::Object(fields) => {
                let old = std::mem::take(fields);
                for (key, mut item) in old {
                    visit(&mut item, token);
                    fields.insert(redact(&key, token), item);
                }
            }
            _ => {}
        }
    }
    visit(&mut value, token);
    value
}

async fn read_limited(response: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        bail!("GitHub response exceeded the {max} byte limit");
    }
    let mut stream = response.bytes_stream();
    let mut output = Vec::new();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("could not read GitHub response")?;
        if output.len().saturating_add(chunk.len()) > max {
            bail!("GitHub response exceeded the {max} byte limit");
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn format_output(value: &Value) -> Result<String> {
    let compact = serde_json::to_string_pretty(value)?;
    Ok(truncate(&compact, MAX_OUTPUT_BYTES))
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_owned();
    }
    let marker = "\n… output truncated";
    let mut end = max.saturating_sub(marker.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_write_tools_are_classified_correctly() {
        let reads: [&dyn Tool; 7] = [
            &GetRepository,
            &ListIssues,
            &ListPullRequests,
            &GetPullRequest,
            &GetPullRequestFiles,
            &ListPullRequestReviews,
            &ListWorkflowRuns,
        ];
        for tool in reads {
            assert!(tool.is_read_only());
            assert!(tool.requires_network());
        }
        let writes: [&dyn Tool; 3] = [&CreateIssue, &CommentOnIssue, &CreatePullRequest];
        for tool in writes {
            assert!(!tool.is_read_only());
            assert!(tool.requires_network());
        }
    }

    #[test]
    fn validates_repo_path_segments_and_pagination() {
        assert_eq!(segment("octocat").unwrap(), "octocat");
        assert!(segment("../evil").is_err());
        assert!(segment("octo/cat").is_err());
        assert!(validate_page(1, 100).is_ok());
        assert!(validate_page(0, 10).is_err());
        assert!(validate_page(1, 101).is_err());
    }

    #[test]
    fn builds_bounded_pagination_query_and_validates_state() {
        let mut path = "/repos/octocat/hello/issues".to_owned();
        append_query(&mut path, 2, 50, Some("closed"));
        assert_eq!(
            path,
            "/repos/octocat/hello/issues?page=2&per_page=50&state=closed"
        );
        assert_eq!(
            validate_state(Some("open")).unwrap().as_deref(),
            Some("open")
        );
        assert!(validate_state(Some("anything")).is_err());
    }

    #[test]
    fn builds_pull_request_files_and_reviews_endpoints_with_pagination() {
        assert_eq!(
            pull_request_files_path("octocat", "hello", 42, 2, 50).unwrap(),
            "/repos/octocat/hello/pulls/42/files?page=2&per_page=50"
        );
        assert_eq!(
            pull_request_reviews_path("octocat", "hello", 42, 1, 30).unwrap(),
            "/repos/octocat/hello/pulls/42/reviews?page=1&per_page=30"
        );
        assert!(pull_request_files_path("../octocat", "hello", 42, 1, 30).is_err());
        assert!(pull_request_reviews_path("octocat", "hello", 0, 1, 30).is_err());
    }

    #[test]
    fn parses_pull_request_review_and_file_inputs() {
        let input: PullRequestListInput =
            parse_input(r#"{"owner":"octocat","repo":"hello","number":42,"page":3,"per_page":75}"#)
                .unwrap();
        assert_eq!(input.number, 42);
        assert_eq!(input.page, 3);
        assert_eq!(input.per_page, 75);
        let defaults: PullRequestListInput =
            parse_input(r#"{"owner":"octocat","repo":"hello","number":42}"#).unwrap();
        assert_eq!(defaults.page, 1);
        assert_eq!(defaults.per_page, 30);
    }

    #[test]
    fn api_base_and_client_never_accept_a_caller_host() {
        let url = api_url("/repos/octocat/hello").unwrap();
        assert_eq!(url.host_str(), Some("api.github.com"));
        assert_eq!(url.scheme(), "https");
        assert!(api_url("//attacker.example/repos/x/y").is_err());
        assert!(api_url("/repos/../attacker").is_err());
    }

    #[test]
    fn output_is_bounded_and_marks_truncation() {
        let input = "x".repeat(MAX_OUTPUT_BYTES + 500);
        let output = truncate(&input, MAX_OUTPUT_BYTES);
        assert!(output.len() <= MAX_OUTPUT_BYTES);
        assert!(output.ends_with("… output truncated"));
    }

    #[test]
    fn parses_mock_github_response_and_redacts_credentials() {
        let token = "ghp_test_secret";
        let response = json!({"full_name":"octocat/hello","description":token});
        let safe = redact_value(response, Some(token));
        assert_eq!(safe["full_name"], "octocat/hello");
        assert_eq!(safe["description"], "[REDACTED]");
        let output = format_output(&safe).unwrap();
        assert!(!output.contains(token));
        assert!(link_has_next(
            "<https://api.github.com/repos/a/b/issues?page=2>; rel=\"next\", <https://api.github.com/repos/a/b/issues?page=4>; rel=\"last\""
        ));
        assert!(!link_has_next(
            "<https://api.github.com/repos/a/b/issues?page=1>; rel=\"prev\""
        ));
    }

    #[test]
    fn filters_pull_requests_from_issue_list_responses() {
        let response = json!([
            {"number":1,"title":"issue"},
            {"number":2,"title":"pull request","pull_request":{"url":"https://api.github.com/..."}}
        ]);
        let filtered = without_pull_requests(response);
        assert_eq!(filtered.as_array().unwrap().len(), 1);
        assert_eq!(filtered[0]["number"], 1);
    }

    #[test]
    fn classifies_primary_and_secondary_rate_limit_responses() {
        assert!(is_rate_limit(StatusCode::FORBIDDEN, Some("0"), false));
        assert!(is_rate_limit(
            StatusCode::TOO_MANY_REQUESTS,
            Some("5"),
            true
        ));
        assert!(!is_rate_limit(StatusCode::FORBIDDEN, Some("5"), false));
    }

    #[test]
    fn parses_github_issue_input() {
        let input: CreateIssueInput =
            parse_input(r#"{"owner":"octocat","repo":"hello","title":"Bug"}"#).unwrap();
        assert_eq!(input.title, "Bug");
        assert_eq!(input.owner, "octocat");
    }
}
