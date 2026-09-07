//! Multi-provider web search engine with auto-fallback.
//!
//! Supports Tavily (AI-agent-native), Brave, Perplexity, SearXNG (self-hosted),
//! Keenable (works without an API key) and DuckDuckGo (HTML scrape fallback).
//! Auto mode cascades through available providers based on configured API keys.
//!
//! All API keys use `Zeroizing<String>` via `resolve_api_key()` to auto-wipe
//! secrets from memory on drop.

use crate::web_cache::WebCache;
use crate::web_content::wrap_external_content;
use openfang_types::config::{SearchProvider, WebConfig};
use std::sync::Arc;
use tracing::{debug, warn};
use zeroize::Zeroizing;

/// Keenable API base URL.
const KEENABLE_API_BASE: &str = "https://api.keenable.ai";
/// Keenable keyed search path (used when the API key is set).
const KEENABLE_SEARCH_PATH: &str = "/v1/search";
/// Keenable public search path (no API key; per-IP rate limits apply).
const KEENABLE_PUBLIC_SEARCH_PATH: &str = "/v1/search/public";
/// Application title the public endpoint requires for attribution.
const KEENABLE_APP_TITLE: &str = "openfang";

/// Multi-provider web search engine.
pub struct WebSearchEngine {
    config: WebConfig,
    client: reqwest::Client,
    cache: Arc<WebCache>,
}

/// Context that bundles both search and fetch engines for passing through the tool runner.
pub struct WebToolsContext {
    pub search: WebSearchEngine,
    pub fetch: crate::web_fetch::WebFetchEngine,
}

impl WebSearchEngine {
    /// Create a new search engine from config with a shared cache.
    pub fn new(config: WebConfig, cache: Arc<WebCache>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            config,
            client,
            cache,
        }
    }

    /// Perform a web search using the configured provider (or auto-fallback).
    pub async fn search(&self, query: &str, max_results: usize) -> Result<String, String> {
        // Check cache first
        let cache_key = format!("search:{}:{}", query, max_results);
        if let Some(cached) = self.cache.get(&cache_key) {
            debug!(query, "Search cache hit");
            return Ok(cached);
        }

        let result = match self.config.search_provider {
            SearchProvider::Brave => self.search_brave(query, max_results).await,
            SearchProvider::Tavily => self.search_tavily(query, max_results).await,
            SearchProvider::Perplexity => self.search_perplexity(query).await,
            SearchProvider::DuckDuckGo => self.search_duckduckgo(query, max_results).await,
            SearchProvider::Searxng => self.search_searxng(query, max_results, None, 1).await,
            SearchProvider::Keenable => self.search_keenable(query, max_results).await,
            SearchProvider::Auto => self.search_auto(query, max_results).await,
        };

        // Cache successful results
        if let Ok(ref content) = result {
            self.cache.put(cache_key, content.clone());
        }

        result
    }

    /// Auto-select provider based on available API keys.
    /// Priority: Tavily → Brave → Perplexity → Searxng → Keenable → DuckDuckGo
    async fn search_auto(&self, query: &str, max_results: usize) -> Result<String, String> {
        // Tavily first (AI-agent-native)
        if resolve_api_key(&self.config.tavily.api_key_env).is_some() {
            debug!("Auto: trying Tavily");
            match self.search_tavily(query, max_results).await {
                Ok(result) => return Ok(result),
                Err(e) => warn!("Tavily failed, falling back: {e}"),
            }
        }

        // Brave second
        if resolve_api_key(&self.config.brave.api_key_env).is_some() {
            debug!("Auto: trying Brave");
            match self.search_brave(query, max_results).await {
                Ok(result) => return Ok(result),
                Err(e) => warn!("Brave failed, falling back: {e}"),
            }
        }

        // Perplexity third
        if resolve_api_key(&self.config.perplexity.api_key_env).is_some() {
            debug!("Auto: trying Perplexity");
            match self.search_perplexity(query).await {
                Ok(result) => return Ok(result),
                Err(e) => warn!("Perplexity failed, falling back: {e}"),
            }
        }

        // Searxng fourth (self-hosted, no API key needed)
        if !self.config.searxng.url.is_empty() {
            debug!("Auto: trying Searxng");
            match self.search_searxng(query, max_results, None, 1).await {
                Ok(result) => return Ok(result),
                Err(e) => warn!("Searxng failed, falling back: {e}"),
            }
        }

        // Keenable fifth (works without an API key; a key only lifts rate limits)
        debug!("Auto: trying Keenable");
        match self.search_keenable(query, max_results).await {
            Ok(result) => return Ok(result),
            Err(e) => warn!("Keenable failed, falling back: {e}"),
        }

        // DuckDuckGo always available as zero-config fallback
        debug!("Auto: falling back to DuckDuckGo");
        self.search_duckduckgo(query, max_results).await
    }

    /// Search via Brave Search API.
    async fn search_brave(&self, query: &str, max_results: usize) -> Result<String, String> {
        let api_key =
            resolve_api_key(&self.config.brave.api_key_env).ok_or("Brave API key not set")?;

        let mut params = vec![("q", query.to_string()), ("count", max_results.to_string())];
        if !self.config.brave.country.is_empty() {
            params.push(("country", self.config.brave.country.clone()));
        }
        if !self.config.brave.search_lang.is_empty() {
            params.push(("search_lang", self.config.brave.search_lang.clone()));
        }
        if !self.config.brave.freshness.is_empty() {
            params.push(("freshness", self.config.brave.freshness.clone()));
        }

        let resp = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&params)
            .header("X-Subscription-Token", api_key.as_str())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("Brave request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Brave API returned {}", resp.status()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Brave JSON parse failed: {e}"))?;

        let results = body["web"]["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        if results.is_empty() {
            return Err(format!("No results found for '{query}' (Brave)."));
        }

        let mut output = format!("Search results for '{query}' (Brave):\n\n");
        for (i, r) in results.iter().enumerate().take(max_results) {
            let title = r["title"].as_str().unwrap_or("");
            let url = r["url"].as_str().unwrap_or("");
            let desc = r["description"].as_str().unwrap_or("");
            output.push_str(&format!(
                "{}. {}\n   URL: {}\n   {}\n\n",
                i + 1,
                title,
                url,
                desc
            ));
        }

        Ok(wrap_external_content("brave-search", &output))
    }

    /// Search via Tavily API (AI-agent-native search).
    async fn search_tavily(&self, query: &str, max_results: usize) -> Result<String, String> {
        let api_key =
            resolve_api_key(&self.config.tavily.api_key_env).ok_or("Tavily API key not set")?;

        let body = serde_json::json!({
            "api_key": api_key.as_str(),
            "query": query,
            "search_depth": self.config.tavily.search_depth,
            "max_results": max_results,
            "include_answer": self.config.tavily.include_answer,
        });

        let resp = self
            .client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Tavily request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Tavily API returned {}", resp.status()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Tavily JSON parse failed: {e}"))?;

        let mut output = format!("Search results for '{query}' (Tavily):\n\n");

        // Include AI-generated answer if available
        if let Some(answer) = data["answer"].as_str() {
            if !answer.is_empty() {
                output.push_str(&format!("AI Summary: {answer}\n\n"));
            }
        }

        let results = data["results"].as_array().cloned().unwrap_or_default();
        for (i, r) in results.iter().enumerate().take(max_results) {
            let title = r["title"].as_str().unwrap_or("");
            let url = r["url"].as_str().unwrap_or("");
            let content = r["content"].as_str().unwrap_or("");
            output.push_str(&format!(
                "{}. {}\n   URL: {}\n   {}\n\n",
                i + 1,
                title,
                url,
                content
            ));
        }

        if results.is_empty() && !output.contains("AI Summary") {
            return Err(format!("No results found for '{query}' (Tavily)."));
        }

        Ok(wrap_external_content("tavily-search", &output))
    }

    /// Search via Perplexity AI (chat completions endpoint).
    async fn search_perplexity(&self, query: &str) -> Result<String, String> {
        let api_key = resolve_api_key(&self.config.perplexity.api_key_env)
            .ok_or("Perplexity API key not set")?;

        let body = serde_json::json!({
            "model": self.config.perplexity.model,
            "messages": [
                {"role": "user", "content": query}
            ],
        });

        let resp = self
            .client
            .post("https://api.perplexity.ai/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key.as_str()))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Perplexity request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Perplexity API returned {}", resp.status()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Perplexity JSON parse failed: {e}"))?;

        let answer = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if answer.is_empty() {
            return Ok(format!("No answer for '{query}' (Perplexity)."));
        }

        let mut output = format!("Search results for '{query}' (Perplexity AI):\n\n{answer}\n");

        // Include citations if available
        if let Some(citations) = data["citations"].as_array() {
            output.push_str("\nSources:\n");
            for (i, c) in citations.iter().enumerate() {
                if let Some(url) = c.as_str() {
                    output.push_str(&format!("  {}. {}\n", i + 1, url));
                }
            }
        }

        Ok(wrap_external_content("perplexity-search", &output))
    }

    /// Search via Keenable (works without an API key).
    ///
    /// Without a key the request goes to the public endpoint, which requires
    /// the `X-Keenable-Title` header and applies per-IP rate limits. With a
    /// key it goes to the keyed endpoint with `X-API-Key`.
    async fn search_keenable(&self, query: &str, max_results: usize) -> Result<String, String> {
        self.search_keenable_at(KEENABLE_API_BASE, query, max_results)
            .await
    }

    /// Search via Keenable against a specific API base URL (tests point this at a local server).
    async fn search_keenable_at(
        &self,
        base_url: &str,
        query: &str,
        max_results: usize,
    ) -> Result<String, String> {
        let api_key = resolve_api_key(&self.config.keenable.api_key_env);

        let path = if api_key.is_some() {
            KEENABLE_SEARCH_PATH
        } else {
            KEENABLE_PUBLIC_SEARCH_PATH
        };

        let body = serde_json::json!({
            "query": query,
            "max_results": max_results.clamp(1, 50),
            "snippet_max_length": self.config.keenable.snippet_max_length,
        });

        let mut request = self
            .client
            .post(format!("{}{}", base_url.trim_end_matches('/'), path))
            .header("Accept", "application/json")
            .header("X-Keenable-Title", KEENABLE_APP_TITLE)
            .json(&body);
        if let Some(ref key) = api_key {
            request = request.header("X-API-Key", key.as_str());
        }

        let resp = request
            .send()
            .await
            .map_err(|e| format!("Keenable request failed: {e}"))?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && api_key.is_none() {
            return Err(format!(
                "Keenable API returned 429 Too Many Requests (public rate limit; set {} to lift it)",
                self.config.keenable.api_key_env
            ));
        }
        if !resp.status().is_success() {
            return Err(format!("Keenable API returned {}", resp.status()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Keenable JSON parse failed: {e}"))?;

        let results = parse_keenable_results(&data, max_results);

        if results.is_empty() {
            return Err(format!("No results found for '{query}' (Keenable)."));
        }

        let mut output = format!("Search results for '{query}' (Keenable):\n\n");
        for (i, (title, url, snippet)) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}\n   URL: {}\n   {}\n\n",
                i + 1,
                title,
                url,
                snippet
            ));
        }

        Ok(wrap_external_content("keenable-search", &output))
    }

    /// Search via DuckDuckGo HTML (no API key needed).
    async fn search_duckduckgo(&self, query: &str, max_results: usize) -> Result<String, String> {
        debug!(query, "Searching via DuckDuckGo HTML");

        let resp = self
            .client
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .header("User-Agent", "Mozilla/5.0 (compatible; OpenFangAgent/0.1)")
            .send()
            .await
            .map_err(|e| format!("DuckDuckGo request failed: {e}"))?;

        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read DDG response: {e}"))?;

        let results = parse_ddg_results(&body, max_results);

        if results.is_empty() {
            return Err(format!("No results found for '{query}'."));
        }

        let mut output = format!("Search results for '{query}':\n\n");
        for (i, (title, url, snippet)) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}\n   URL: {}\n   {}\n\n",
                i + 1,
                title,
                url,
                snippet
            ));
        }

        Ok(output)
    }

    /// Search via SearXNG self-hosted instance.
    async fn search_searxng(
        &self,
        query: &str,
        max_results: usize,
        category: Option<&str>,
        page: u32,
    ) -> Result<String, String> {
        if self.config.searxng.url.is_empty() {
            return Err("SearXNG URL is not configured".to_string());
        }

        let category = category.unwrap_or("general");

        // Validate category against SearXNG instance
        match self.list_searxng_categories().await {
            Ok(cats) => {
                if !cats.iter().any(|c| c == category) {
                    return Err(format!(
                        "Invalid SearXNG category '{}'. Available: {}",
                        category,
                        cats.join(", ")
                    ));
                }
            }
            Err(e) => warn!("Could not validate SearXNG category: {e}"),
        }

        let limit = max_results;

        debug!(query, "Searching via SearXNG");

        let resp = self
            .client
            .get(format!(
                "{}/search",
                self.config.searxng.url.trim_end_matches('/')
            ))
            .query(&[
                ("q", query),
                ("format", "json"),
                ("categories", category),
                ("page", &page.to_string()),
            ])
            .header("User-Agent", "Mozilla/5.0 (compatible; OpenFangAgent/0.1)")
            .send()
            .await
            .map_err(|e| format!("SearXNG request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("SearXNG API returned {}", resp.status()));
        }

        #[derive(serde::Deserialize)]
        struct SearxngResponse {
            results: Vec<SearxngResult>,
            query: String,
        }

        #[derive(serde::Deserialize)]
        struct SearxngResult {
            url: String,
            title: String,
            content: Option<String>,
            #[serde(alias = "pubdate")]
            published_date: Option<String>,
        }

        let data: SearxngResponse = resp
            .json()
            .await
            .map_err(|e| format!("SearXNG JSON parse failed: {e}"))?;

        if data.results.is_empty() {
            return Err(format!("No results found for '{query}' (SearXNG)."));
        }

        #[derive(serde::Serialize)]
        struct OutputResult<'a> {
            title: &'a str,
            url: &'a str,
            content: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            published_date: Option<&'a str>,
        }

        #[derive(serde::Serialize)]
        struct Output<'a> {
            query: &'a str,
            results: Vec<OutputResult<'a>>,
        }

        let results: Vec<OutputResult> = data
            .results
            .iter()
            .take(limit)
            .map(|r| OutputResult {
                title: &r.title,
                url: &r.url,
                content: r.content.as_deref().unwrap_or(""),
                published_date: r.published_date.as_deref(),
            })
            .collect();

        let output = Output {
            query: &data.query,
            results,
        };

        serde_json::to_string(&output)
            .map_err(|e| format!("Failed to serialize SearXNG results: {e}"))
    }

    /// List available search categories from the SearXNG instance.
    ///
    /// Fetches the `/config` endpoint and returns the list of categories
    /// the instance supports (e.g., "general", "images", "news", "videos").
    /// Returns an error if SearXNG URL is not configured or the request fails.
    pub async fn list_searxng_categories(&self) -> Result<Vec<String>, String> {
        if self.config.searxng.url.is_empty() {
            return Err("SearXNG URL is not configured".to_string());
        }

        #[derive(serde::Deserialize)]
        struct SearxngConfig {
            categories: Vec<String>,
        }

        let resp = self
            .client
            .get(format!(
                "{}/config",
                self.config.searxng.url.trim_end_matches('/')
            ))
            .header("User-Agent", "Mozilla/5.0 (compatible; OpenFangAgent/0.1)")
            .send()
            .await
            .map_err(|e| format!("SearXNG config request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("SearXNG config API returned {}", resp.status()));
        }

        let data: SearxngConfig = resp
            .json()
            .await
            .map_err(|e| format!("SearXNG config JSON parse failed: {e}"))?;

        Ok(data.categories)
    }

    /// Return all non-Auto search providers (for provider listing/discovery).
    pub fn all_providers() -> Vec<SearchProvider> {
        vec![
            SearchProvider::Brave,
            SearchProvider::Tavily,
            SearchProvider::Perplexity,
            SearchProvider::DuckDuckGo,
            SearchProvider::Searxng,
            SearchProvider::Keenable,
        ]
    }
}

// ---------------------------------------------------------------------------
// Keenable JSON parser
// ---------------------------------------------------------------------------

/// Parse a Keenable search response into (title, url, snippet) tuples.
///
/// Reads `snippet` first and falls back to `description`, which the API
/// usually leaves empty. Results without a URL are skipped.
pub fn parse_keenable_results(
    data: &serde_json::Value,
    max: usize,
) -> Vec<(String, String, String)> {
    let Some(results) = data["results"].as_array() else {
        return Vec::new();
    };

    results
        .iter()
        .filter_map(|r| {
            let url = r["url"].as_str().unwrap_or("").trim();
            if url.is_empty() {
                return None;
            }
            let title = r["title"].as_str().unwrap_or("").trim();
            let snippet = match r["snippet"].as_str().map(str::trim) {
                Some(s) if !s.is_empty() => s,
                _ => r["description"].as_str().unwrap_or("").trim(),
            };
            Some((title.to_string(), url.to_string(), snippet.to_string()))
        })
        .take(max)
        .collect()
}

// ---------------------------------------------------------------------------
// DuckDuckGo HTML parser (moved from tool_runner.rs)
// ---------------------------------------------------------------------------

/// Parse DuckDuckGo HTML search results into (title, url, snippet) tuples.
pub fn parse_ddg_results(html: &str, max: usize) -> Vec<(String, String, String)> {
    let mut results = Vec::new();

    for chunk in html.split("class=\"result__a\"") {
        if results.len() >= max {
            break;
        }
        if !chunk.contains("href=") {
            continue;
        }

        let url = extract_between(chunk, "href=\"", "\"")
            .unwrap_or_default()
            .to_string();

        let actual_url = if url.contains("uddg=") {
            url.split("uddg=")
                .nth(1)
                .and_then(|u| u.split('&').next())
                .map(urldecode)
                .unwrap_or(url)
        } else {
            url
        };

        let title = extract_between(chunk, ">", "</a>")
            .map(strip_html_tags)
            .unwrap_or_default();

        let snippet = if let Some(snip_start) = chunk.find("class=\"result__snippet\"") {
            let after = &chunk[snip_start..];
            extract_between(after, ">", "</a>")
                .or_else(|| extract_between(after, ">", "</"))
                .map(strip_html_tags)
                .unwrap_or_default()
        } else {
            String::new()
        };

        if !title.is_empty() && !actual_url.is_empty() {
            results.push((title, actual_url, snippet));
        }
    }

    results
}

/// Extract text between two delimiters.
pub fn extract_between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_idx = text.find(start)? + start.len();
    let remaining = &text[start_idx..];
    let end_idx = remaining.find(end)?;
    Some(&remaining[..end_idx])
}

/// Strip HTML tags from a string.
pub fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
}

/// Simple percent-decode for URLs.
pub fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if ch == '+' {
            result.push(' ');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Resolve an API key from an environment variable name.
/// Returns `Zeroizing<String>` that auto-wipes from memory on drop.
fn resolve_api_key(env_var: &str) -> Option<Zeroizing<String>> {
    std::env::var(env_var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(Zeroizing::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_with_results() {
        let html = r#"junk class="result__a" href="https://example.com">Example</a> class="result__snippet">A snippet</a>"#;
        let results = parse_ddg_results(html, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "Example");
        assert_eq!(results[0].1, "https://example.com");
        assert_eq!(results[0].2, "A snippet");
    }

    #[test]
    fn test_format_empty() {
        let results = parse_ddg_results("<html><body>No results</body></html>", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_format_with_answer() {
        // Tavily-style answer formatting is tested via the DDG parser as basic coverage
        let html = r#"before class="result__a" href="https://rust-lang.org">Rust</a> class="result__snippet">Systems programming</a> class="result__a" href="https://go.dev">Go</a> class="result__snippet">Another language</a>"#;
        let results = parse_ddg_results(html, 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_ddg_parser_preserved() {
        // Ensure the parser handles URL-encoded DDG redirect URLs
        let html = r#"x class="result__a" href="/l/?uddg=https%3A%2F%2Fexample.com&rut=abc">Title</a> class="result__snippet">Desc</a>"#;
        let results = parse_ddg_results(html, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "https://example.com");
    }

    // -----------------------------------------------------------------------
    // Keenable
    // -----------------------------------------------------------------------

    #[test]
    fn test_keenable_parser_reads_snippet_first() {
        let data = serde_json::json!({
            "results": [
                {
                    "url": "https://www.rust-lang.org/",
                    "title": "Rust Programming Language",
                    "snippet": "A language empowering everyone to build reliable software.",
                    "description": "ignored when snippet is present"
                },
                {
                    "url": "https://doc.rust-lang.org/book/",
                    "title": "The Rust Book",
                    "snippet": "",
                    "description": "Falls back to description when snippet is empty"
                },
                {
                    "url": "https://example.com/no-text",
                    "title": "No text at all"
                },
                {
                    "title": "Dropped: no url",
                    "snippet": "irrelevant"
                }
            ]
        });
        let results = parse_keenable_results(&data, 10);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "Rust Programming Language");
        assert_eq!(results[0].1, "https://www.rust-lang.org/");
        assert_eq!(
            results[0].2,
            "A language empowering everyone to build reliable software."
        );
        assert_eq!(
            results[1].2,
            "Falls back to description when snippet is empty"
        );
        assert_eq!(results[2].2, "");
    }

    #[test]
    fn test_keenable_parser_respects_max_and_empty() {
        let data = serde_json::json!({
            "results": [
                {"url": "https://a.example", "title": "A", "snippet": "a"},
                {"url": "https://b.example", "title": "B", "snippet": "b"},
                {"url": "https://c.example", "title": "C", "snippet": "c"}
            ]
        });
        assert_eq!(parse_keenable_results(&data, 2).len(), 2);
        assert!(parse_keenable_results(&serde_json::json!({"results": []}), 5).is_empty());
        assert!(parse_keenable_results(&serde_json::json!({"error": "nope"}), 5).is_empty());
        assert!(parse_keenable_results(&serde_json::json!("not an object"), 5).is_empty());
    }

    /// Minimal one-shot HTTP server: accepts a single request, returns the
    /// canned status + JSON body, and hands back the raw request text so the
    /// test can assert on method, path, headers, and body.
    async fn one_shot_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    let headers = &text[..header_end];
                    let content_length = headers
                        .lines()
                        .find_map(|l| {
                            let (name, value) = l.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if buf.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.ok();
            String::from_utf8_lossy(&buf).into_owned()
        });
        (format!("http://{addr}"), handle)
    }

    fn engine_with_key_env(env_var: &str) -> WebSearchEngine {
        let mut config = WebConfig::default();
        config.keenable.api_key_env = env_var.to_string();
        WebSearchEngine::new(
            config,
            Arc::new(WebCache::new(std::time::Duration::from_secs(60))),
        )
    }

    fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.eq_ignore_ascii_case(name).then_some(v.trim())
        })
    }

    const KEENABLE_OK_BODY: &str = r#"{"results":[
        {"url":"https://www.rust-lang.org/","title":"Rust","snippet":"A language empowering everyone.","description":""},
        {"url":"https://doc.rust-lang.org/book/","title":"The Book","snippet":"","description":"Only a description here."}
    ]}"#;

    #[tokio::test]
    async fn test_keenable_keyless_uses_public_endpoint_and_title_header() {
        let env_var = "OPENFANG_TEST_KEENABLE_KEY_UNSET";
        std::env::remove_var(env_var);
        let (base_url, server) = one_shot_server("200 OK", KEENABLE_OK_BODY).await;

        let engine = engine_with_key_env(env_var);
        let output = engine
            .search_keenable_at(&base_url, "rust programming language", 5)
            .await
            .unwrap();

        let request = server.await.unwrap();
        assert!(
            request.starts_with("POST /v1/search/public HTTP/1.1"),
            "keyless request must hit the public endpoint: {request}"
        );
        assert_eq!(header_value(&request, "X-Keenable-Title"), Some("openfang"));
        assert!(header_value(&request, "X-API-Key").is_none());
        let body_start = request.find("\r\n\r\n").unwrap() + 4;
        let body: serde_json::Value = serde_json::from_str(&request[body_start..]).unwrap();
        assert_eq!(body["query"], "rust programming language");
        assert_eq!(body["max_results"], 5);
        assert_eq!(body["snippet_max_length"], 500);

        assert!(output.contains("Search results for 'rust programming language' (Keenable):"));
        assert!(output.contains(
            "1. Rust\n   URL: https://www.rust-lang.org/\n   A language empowering everyone."
        ));
        assert!(output.contains(
            "2. The Book\n   URL: https://doc.rust-lang.org/book/\n   Only a description here."
        ));
    }

    #[tokio::test]
    async fn test_keenable_keyed_uses_search_endpoint_and_api_key_header() {
        let env_var = "OPENFANG_TEST_KEENABLE_KEY_SET";
        std::env::set_var(env_var, "kn-test-key-123");
        let (base_url, server) = one_shot_server("200 OK", KEENABLE_OK_BODY).await;

        let engine = engine_with_key_env(env_var);
        let output = engine
            .search_keenable_at(&base_url, "rust", 60)
            .await
            .unwrap();
        std::env::remove_var(env_var);

        let request = server.await.unwrap();
        assert!(
            request.starts_with("POST /v1/search HTTP/1.1"),
            "keyed request must hit the keyed endpoint: {request}"
        );
        assert_eq!(header_value(&request, "X-API-Key"), Some("kn-test-key-123"));
        assert_eq!(header_value(&request, "X-Keenable-Title"), Some("openfang"));
        let body_start = request.find("\r\n\r\n").unwrap() + 4;
        let body = &request[body_start..];
        assert!(
            !body.contains("kn-test-key-123"),
            "key must travel in a header, not the body"
        );
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            body["max_results"], 50,
            "max_results is clamped to the API ceiling"
        );
        assert!(output.contains("(Keenable)"));
    }

    #[tokio::test]
    async fn test_keenable_rate_limit_is_an_error_not_empty_success() {
        let env_var = "OPENFANG_TEST_KEENABLE_KEY_UNSET_429";
        std::env::remove_var(env_var);
        let (base_url, server) =
            one_shot_server("429 Too Many Requests", r#"{"error":"rate limited"}"#).await;

        let engine = engine_with_key_env(env_var);
        let err = engine
            .search_keenable_at(&base_url, "rust", 5)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert!(err.contains("429"), "{err}");
        assert!(
            err.contains(env_var),
            "the error should name the env var that lifts the limit: {err}"
        );
    }

    #[tokio::test]
    async fn test_keenable_no_results_is_an_error() {
        let env_var = "OPENFANG_TEST_KEENABLE_KEY_UNSET_EMPTY";
        std::env::remove_var(env_var);
        let (base_url, server) = one_shot_server("200 OK", r#"{"results":[]}"#).await;

        let engine = engine_with_key_env(env_var);
        let err = engine
            .search_keenable_at(&base_url, "zzqx", 5)
            .await
            .unwrap_err();
        server.await.unwrap();
        assert_eq!(err, "No results found for 'zzqx' (Keenable).");
    }

    #[test]
    fn test_all_providers_lists_keenable() {
        assert!(WebSearchEngine::all_providers().contains(&SearchProvider::Keenable));
    }
}
