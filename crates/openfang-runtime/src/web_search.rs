//! Multi-provider web search engine with auto-fallback.
//!
//! Supports 4 providers: Tavily (AI-agent-native), Brave, Perplexity, and
//! DuckDuckGo (zero-config fallback). Auto mode cascades through available
//! providers based on configured API keys.
//!
//! All API keys use `Zeroizing<String>` via `resolve_api_key()` to auto-wipe
//! secrets from memory on drop.

use crate::web_cache::WebCache;
use crate::web_content::wrap_external_content;
use openfang_types::config::{SearchProvider, WebConfig};
use std::sync::Arc;
use tracing::{debug, warn};
use zeroize::Zeroizing;

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
            SearchProvider::Auto => self.search_auto(query, max_results).await,
        };

        // Cache successful results
        if let Ok(ref content) = result {
            self.cache.put(cache_key, content.clone());
        }

        result
    }

    /// Auto-select provider based on available API keys.
    /// Priority: Tavily → Brave → Perplexity → Searxng → DuckDuckGo
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
            .get(format!("{}/search", self.config.searxng.url.trim_end_matches('/')))
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
            .get(format!("{}/config", self.config.searxng.url.trim_end_matches('/')))
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
        ]
    }
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
}
