//! # Search Tools
//!
//! Tools for web search and crate discovery.

use radkit::macros::tool;
use radkit::tools::ToolResult;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

/// Arguments for web search
#[derive(Deserialize, JsonSchema)]
pub struct SearchWebArgs {
    /// Search query
    pub query: String,
    /// Maximum number of results (default: 5)
    pub max_results: Option<u32>,
}

/// Search the web for information
#[tool(
    description = "Search the web for information. Returns search results with URLs and snippets."
)]
pub async fn search_web(args: SearchWebArgs) -> ToolResult {
    let max_results = args.max_results.unwrap_or(5);

    // Try SearXNG first (self-hosted search)
    let searxng_result = try_searxng(&args.query, max_results).await;

    if let Some(results) = searxng_result {
        return ToolResult::success(json!({
            "query": args.query,
            "source": "searxng",
            "results": results
        }));
    }

    // Fallback: Return that we couldn't search
    ToolResult::success(json!({
        "query": args.query,
        "source": "none",
        "results": [],
        "message": "No search backend available. Consider installing SearXNG."
    }))
}

async fn try_searxng(query: &str, max_results: u32) -> Option<Vec<serde_json::Value>> {
    // Build list of endpoints to try:
    // 1. SEARXNG_URL env var (user configured)
    // 2. Public instances
    // 3. Local fallback
    let mut endpoints: Vec<String> = Vec::new();

    // User-configured via env var takes priority
    if let Ok(custom_url) = std::env::var("SEARXNG_URL") {
        endpoints.push(format!("{}/search", custom_url.trim_end_matches('/')));
    }

    // Public SearXNG instances (subset of reliable ones)
    // Full list: https://searx.space/
    endpoints.extend([
        "https://searx.be/search".to_string(),
        "https://search.sapti.me/search".to_string(),
        "https://searx.tiekoetter.com/search".to_string(),
    ]);

    // Local fallback
    endpoints.push("http://localhost:8888/search".to_string());
    endpoints.push("http://127.0.0.1:8888/search".to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    for endpoint in endpoints {
        let url = format!("{}?q={}&format=json", endpoint, urlencoding::encode(query));

        if let Ok(response) = client.get(&url).send().await {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
                    let limited: Vec<serde_json::Value> = results
                        .iter()
                        .take(max_results as usize)
                        .map(|r| {
                            json!({
                                "title": r.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                                "url": r.get("url").and_then(|u| u.as_str()).unwrap_or(""),
                                "snippet": r.get("content").and_then(|c| c.as_str()).unwrap_or("")
                            })
                        })
                        .collect();
                    return Some(limited);
                }
            }
        }
    }

    None
}

/// Arguments for crates.io search
#[derive(Deserialize, JsonSchema)]
pub struct SearchCratesArgs {
    /// Search query
    pub query: String,
    /// Maximum number of results (default: 10)
    pub max_results: Option<u32>,
}

/// Search crates.io for Rust crates
#[tool(
    description = "Search crates.io for Rust crates. Returns crate names, versions, and descriptions."
)]
pub async fn search_crates(args: SearchCratesArgs) -> ToolResult {
    let max_results = args.max_results.unwrap_or(10);
    let url = format!(
        "https://crates.io/api/v1/crates?q={}&per_page={}",
        urlencoding::encode(&args.query),
        max_results
    );

    let client = reqwest::Client::builder()
        .user_agent("catalyst-agent/1.0")
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => return ToolResult::error(format!("Failed to create HTTP client: {}", e)),
    };

    match client.get(&url).send().await {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(json) => {
                let crates: Vec<serde_json::Value> = json
                    .get("crates")
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|c| {
                                json!({
                                    "name": c.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                    "version": c.get("max_version").and_then(|v| v.as_str()).unwrap_or(""),
                                    "description": c.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                                    "downloads": c.get("downloads").and_then(|d| d.as_u64()).unwrap_or(0),
                                    "documentation": c.get("documentation").and_then(|d| d.as_str())
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                ToolResult::success(json!({
                    "query": args.query,
                    "crates": crates
                }))
            }
            Err(e) => ToolResult::error(format!("Failed to parse crates.io response: {}", e)),
        },
        Err(e) => ToolResult::error(format!("Failed to query crates.io: {}", e)),
    }
}
