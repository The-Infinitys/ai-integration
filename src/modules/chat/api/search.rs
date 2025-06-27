// src/modules/chat/api/search.rs

use reqwest::Client;
use std::error::Error;
use std::io::{self, Write};
use html2md::parse_html;
use urlencoding;
use crate::dprintln; // Import the dprintln macro
use scraper::{Html, Selector}; // Import Html and Selector from scraper

/// Executes a real web search or directly accesses a URL, parses the HTML to Markdown, and returns it.
///
/// # Arguments
/// * `client` - The `reqwest::Client` instance to use for HTTP requests.
/// * `debug_mode` - A flag to enable or disable debug output.
/// * `query` - The query string for Google search (optional).
/// * `url` - The URL to directly access (optional).
/// * `engine` - The search engine to use (e.g., "google"). Only relevant if `query` is provided.
///
/// # Returns
/// `Result<String, Box<dyn Error>>` - The search/access result in Markdown format, or an error.
pub async fn execute_web_search(
    client: &Client,
    debug_mode: bool,
    query: Option<&str>,
    url: Option<&str>,
    engine: Option<&str>,
) -> Result<String, Box<dyn Error>> {
    let fetch_url;
    let action_description: String;
    let mut is_google_search = false;

    if let Some(target_url) = url {
        // If a URL is specified, access it directly.
        fetch_url = target_url.to_string();
        action_description = format!("URLアクセス: '{}'", target_url);
    } else if let Some(search_query) = query {
        // If a query is specified, perform a Google search.
        let used_engine = engine.unwrap_or("google");
        // Build the Google search URL
        fetch_url = format!("https://www.google.com/search?q={}", urlencoding::encode(search_query));
        action_description = format!("{}検索: '{}'", used_engine, search_query);
        is_google_search = true;
    } else {
        return Err("web_searchツールには 'query' または 'url' のいずれかが必要です。".into());
    }

    dprintln!(
        debug_mode,
        "\n[AI (ツール): {} を実行中... URL: {}]",
        action_description,
        fetch_url
    );
    io::stdout().flush().unwrap_or_default();

    let response = client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("Webリクエストの送信に失敗しました: {}. URL: {}", e, fetch_url))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "Webリクエストが失敗しました。ステータス: {}, ボディ: {}. URL: {}",
            status, text, fetch_url
        )
        .into());
    }

    let html_content = response.text().await.map_err(|e| {
        format!(
            "HTMLコンテンツの取得に失敗しました: {}. URL: {}",
            e, fetch_url
        )
    })?;

    let markdown_content: String;

    if is_google_search {
        // Google検索結果ページから主要な結果を抽出してMarkdownに整形
        dprintln!(debug_mode, "[AI (ツール): Google検索結果のHTMLをパース中...]");
        let document = Html::parse_document(&html_content);

        // Googleの検索結果の主要な要素をターゲットとするCSSセレクタ
        // これらのセレクタはGoogleのHTML構造に依存し、変更される可能性があります。
        let result_selector = Selector::parse("div.g").unwrap(); // Individual search result blocks
        let title_selector = Selector::parse("h3.LC20lb.MBeuO.DKV0Md").unwrap(); // Title of a result
        let link_selector = Selector::parse("div.yuRUbf a").unwrap(); // Link element
        let snippet_selector = Selector::parse("div.VwiC3b").unwrap(); // Snippet/description

        let mut results_md = String::new();
        results_md.push_str("### Google検索結果:\n");

        for (i, element) in document.select(&result_selector).enumerate() {
            let title = element.select(&title_selector).next().map(|n| n.text().collect::<String>());
            let link = element.select(&link_selector).next().and_then(|n| n.value().attr("href"));
            let snippet = element.select(&snippet_selector).next().map(|n| n.text().collect::<String>());

            if let (Some(t), Some(l)) = (title, link) {
                results_md.push_str(&format!("{}. [{}]({})\n", i + 1, t, l));
                if let Some(s) = snippet {
                    results_md.push_str(&format!("   {}\n", s.trim()));
                }
                results_md.push_str("\n");
            }
        }

        if results_md.len() <= "### Google検索結果:\n".len() {
            // 特定の要素が見つからなかった場合、ページ全体をMarkdownに変換するフォールバック
            dprintln!(debug_mode, "[AI (ツール): 特定のGoogle検索結果要素が見つかりませんでした。ページ全体をMarkdownに変換します。]");
            markdown_content = parse_html(&html_content);
        } else {
            markdown_content = results_md;
        }

    } else {
        // 通常のWebページアクセスの場合、ページ全体をMarkdownに変換
        dprintln!(debug_mode, "[AI (ツール): Webページ全体をMarkdownに変換中...]");
        markdown_content = parse_html(&html_content);
    }

    // トークン制限を考慮して結果を切り捨てる
    let truncated_markdown = if markdown_content.len() > 4000 {
        format!(
            "{}\n...(結果は長すぎるため一部省略されました)",
            &markdown_content[..4000]
        )
    } else {
        markdown_content
    };

    dprintln!(
        debug_mode,
        "[AI (ツール): 検索/アクセス結果のHTMLをMarkdownに変換しました。]"
    );
    io::stdout().flush().unwrap_or_default();

    Ok(format!(
        "{}結果:\n```markdown\n{}\n```\n",
        action_description, truncated_markdown
    ))
}
