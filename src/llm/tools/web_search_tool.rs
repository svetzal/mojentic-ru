use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const BASE_URL: &str = "https://lite.duckduckgo.com/lite/";
const MAX_RESULTS: usize = 10;
const TIMEOUT_SECONDS: u64 = 10;

/// A web search result from DuckDuckGo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    /// The title of the search result
    pub title: String,
    /// The URL of the search result
    pub url: String,
    /// A snippet/description of the search result
    pub snippet: String,
}

/// Tool for searching the web using DuckDuckGo
///
/// This tool searches DuckDuckGo's lite endpoint and returns organic search results.
/// It does not require an API key, making it a free alternative to paid search APIs.
///
/// # Examples
///
/// ```ignore
/// use mojentic::llm::tools::web_search_tool::WebSearchTool;
/// use std::collections::HashMap;
///
/// let tool = WebSearchTool::new();
/// let mut args = HashMap::new();
/// args.insert("query".to_string(), serde_json::json!("Rust programming"));
///
/// let results = tool.run(&args)?;
/// // results contains an array of search results with title, url, and snippet
/// ```
#[derive(Clone)]
pub struct WebSearchTool {
    client: reqwest::Client,
}

impl WebSearchTool {
    /// Creates a new WebSearchTool instance
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECONDS))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Creates a new WebSearchTool with a custom HTTP client (for testing)
    #[cfg(test)]
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Perform the web search
    async fn perform_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let url = format!("{}?q={}", BASE_URL, urlencoding::encode(query));

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(crate::error::MojenticError::HttpError)?;

        if !response.status().is_success() {
            return Err(crate::error::MojenticError::ApiError(format!(
                "HTTP request failed with status {}",
                response.status()
            )));
        }

        let html = response.text().await.map_err(crate::error::MojenticError::HttpError)?;

        self.parse_results(&html)
    }

    /// Parse HTML results from DuckDuckGo lite
    fn parse_results(&self, html: &str) -> Result<Vec<SearchResult>> {
        let document = Html::parse_document(html);

        // DuckDuckGo lite uses a simple structure with result-link class
        let link_selector = Selector::parse("a.result-link").map_err(|e| {
            crate::error::MojenticError::ParseError(format!("Invalid selector: {:?}", e))
        })?;
        let snippet_selector = Selector::parse("td.result-snippet").map_err(|e| {
            crate::error::MojenticError::ParseError(format!("Invalid selector: {:?}", e))
        })?;

        let mut results = Vec::new();

        // Extract links and titles
        let links: Vec<_> = document.select(&link_selector).collect();
        let snippets: Vec<_> = document.select(&snippet_selector).collect();

        for (i, link) in links.iter().take(MAX_RESULTS).enumerate() {
            if let Some(href) = link.value().attr("href") {
                let title = link.text().collect::<Vec<_>>().join(" ");
                let url = Self::decode_url(href);

                let snippet = snippets
                    .get(i)
                    .map(|s| Self::clean_text(&s.text().collect::<Vec<_>>().join(" ")))
                    .unwrap_or_default();

                results.push(SearchResult {
                    title: Self::clean_text(&title),
                    url,
                    snippet,
                });
            }
        }

        Ok(results)
    }

    /// Decode DuckDuckGo redirect URLs
    fn decode_url(url: &str) -> String {
        // DuckDuckGo uses redirect URLs like //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com
        if url.contains("uddg=") {
            url.split("uddg=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
                .unwrap_or_else(|| url.to_string())
        } else {
            url.to_string()
        }
    }

    /// Clean text by removing extra whitespace and decoding HTML entities
    fn clean_text(text: &str) -> String {
        let text = text.trim().replace(|c: char| c.is_whitespace(), " ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");

        // Decode common HTML entities
        text.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmTool for WebSearchTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            crate::error::MojenticError::InvalidArgument("query parameter is required".to_string())
        })?;

        if query.is_empty() {
            return Err(crate::error::MojenticError::InvalidArgument(
                "query parameter cannot be empty".to_string(),
            ));
        }

        // Since we're in a sync context, we need to run the async function
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            crate::error::MojenticError::RuntimeError(format!("Failed to create runtime: {}", e))
        })?;

        let results = rt.block_on(self.perform_search(query)).map_err(|e| {
            crate::error::MojenticError::ToolExecutionError(format!("Search failed: {}", e))
        })?;

        Ok(json!(results))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "web_search".to_string(),
                description: "Search the web for information using DuckDuckGo. Returns organic search results including title, URL, and snippet for each result.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn sample_html() -> String {
        r#"
        <!DOCTYPE html>
        <html>
        <body>
            <table>
                <tr>
                    <td>
                        <a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F">The Rust Programming Language</a>
                    </td>
                </tr>
                <tr>
                    <td class="result-snippet">A language empowering everyone to build reliable and efficient software.</td>
                </tr>
                <tr>
                    <td>
                        <a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust-lang.org%2F">Rust Documentation</a>
                    </td>
                </tr>
                <tr>
                    <td class="result-snippet">The official Rust documentation and learning resources.</td>
                </tr>
                <tr>
                    <td>
                        <a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fcrates.io%2F">crates.io: Rust Package Registry</a>
                    </td>
                </tr>
                <tr>
                    <td class="result-snippet">The Rust community's crate registry.</td>
                </tr>
            </table>
        </body>
        </html>
        "#.to_string()
    }

    #[test]
    fn test_descriptor() {
        let tool = WebSearchTool::new();
        let descriptor = tool.descriptor();

        assert_eq!(descriptor.r#type, "function");
        assert_eq!(descriptor.function.name, "web_search");
        assert!(descriptor
            .function
            .description
            .contains("Search the web for information using DuckDuckGo"));

        let params = descriptor.function.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["query"].is_object());
        assert_eq!(params["required"][0], "query");
    }

    #[test]
    fn test_parse_results() {
        let tool = WebSearchTool::new();
        let html = sample_html();

        let results = tool.parse_results(&html).unwrap();

        assert_eq!(results.len(), 3);

        assert_eq!(results[0].title, "The Rust Programming Language");
        assert_eq!(results[0].url, "https://www.rust-lang.org/");
        assert!(results[0].snippet.contains("A language empowering everyone"));

        assert_eq!(results[1].title, "Rust Documentation");
        assert_eq!(results[1].url, "https://doc.rust-lang.org/");
        assert!(results[1].snippet.contains("official Rust documentation"));

        assert_eq!(results[2].title, "crates.io: Rust Package Registry");
        assert_eq!(results[2].url, "https://crates.io/");
        assert!(results[2].snippet.contains("crate registry"));
    }

    #[test]
    fn test_decode_url() {
        let url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath";
        let decoded = WebSearchTool::decode_url(url);
        assert_eq!(decoded, "https://example.com/path");

        // Test URL without encoding
        let url = "https://example.com/direct";
        let decoded = WebSearchTool::decode_url(url);
        assert_eq!(decoded, "https://example.com/direct");
    }

    #[test]
    fn test_clean_text() {
        assert_eq!(WebSearchTool::clean_text("  Multiple   spaces  "), "Multiple spaces");
        assert_eq!(WebSearchTool::clean_text("Text&amp;more"), "Text&more");
        assert_eq!(WebSearchTool::clean_text("&lt;tag&gt;"), "<tag>");
        assert_eq!(WebSearchTool::clean_text("&quot;quoted&quot;"), "\"quoted\"");
        assert_eq!(WebSearchTool::clean_text("it&#39;s"), "it's");
    }

    #[test]
    fn test_tool_matches() {
        let tool = WebSearchTool::new();
        assert!(tool.matches("web_search"));
        assert!(!tool.matches("other_tool"));
    }

    #[test]
    fn test_run_missing_query() {
        let tool = WebSearchTool::new();
        let args = HashMap::new();

        let result = tool.run(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query parameter is required"));
    }

    #[test]
    fn test_run_empty_query() {
        let tool = WebSearchTool::new();
        let mut args = HashMap::new();
        args.insert("query".to_string(), json!(""));

        let result = tool.run(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query parameter cannot be empty"));
    }

    #[tokio::test]
    async fn test_perform_search_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(sample_html())
            .create_async()
            .await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();

        let tool = WebSearchTool::with_client(client);

        // Override BASE_URL for testing by constructing URL directly
        let url = format!("{}?q=rust", server.url());
        let response = tool.client.get(&url).send().await.unwrap();
        let html = response.text().await.unwrap();
        let results = tool.parse_results(&html).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "The Rust Programming Language");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_perform_search_http_error() {
        let mut server = Server::new_async().await;
        let mock = server.mock("GET", mockito::Matcher::Any).with_status(500).create_async().await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();

        let tool = WebSearchTool::with_client(client);

        let url = format!("{}?q=test", server.url());
        let response = tool.client.get(&url).send().await.unwrap();

        assert_eq!(response.status(), 500);

        mock.assert_async().await;
    }

    #[test]
    fn test_max_results_limit() {
        let tool = WebSearchTool::new();

        // Create HTML with more than MAX_RESULTS entries
        let mut html = String::from("<html><body><table>");
        for i in 0..15 {
            html.push_str(&format!(
                r#"<tr><td><a class="result-link" href="https://example.com/{}">Result {}</a></td></tr>"#,
                i, i
            ));
            html.push_str(&format!(r#"<tr><td class="result-snippet">Snippet {}</td></tr>"#, i));
        }
        html.push_str("</table></body></html>");

        let results = tool.parse_results(&html).unwrap();

        assert_eq!(results.len(), MAX_RESULTS);
    }

    #[test]
    fn test_clone_box() {
        let tool = WebSearchTool::new();
        let cloned = tool.clone_box();

        assert_eq!(cloned.descriptor().function.name, tool.descriptor().function.name);
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            title: "Test Title".to_string(),
            url: "https://example.com".to_string(),
            snippet: "Test snippet".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test Title"));
        assert!(json.contains("https://example.com"));
        assert!(json.contains("Test snippet"));

        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, result);
    }

    #[test]
    fn test_parse_empty_html() {
        let tool = WebSearchTool::new();
        let html = "<html><body></body></html>";

        let results = tool.parse_results(html).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_parse_malformed_html() {
        let tool = WebSearchTool::new();
        let html = "<html><body><a class=\"result-link\">No href</a></body></html>";

        let results = tool.parse_results(html).unwrap();
        assert_eq!(results.len(), 0);
    }
}
