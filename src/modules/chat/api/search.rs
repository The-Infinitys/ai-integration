// src/modules/chat/api/search.rs

use crate::dprintln; // dprintlnマクロをインポート
use html2md::parse_html;
use reqwest::Client;
use scraper::{Html, Selector};
use std::error::Error;
use std::io::{self, Write};
use urlencoding; // HtmlとSelectorをインポート

/// Represents a single search result with its title, URL, and description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
}

/// 実際のWeb検索またはURLアクセスを実行し、HTMLをMarkdownにパースして返す
///
/// # Arguments
/// * `client` - HTTPリクエストに使用するreqwest::Clientインスタンス。
/// * `debug_mode` - デバッグ出力を有効にするかどうか。
/// * `query` - Google検索に使用するクエリ（オプション）。
/// * `url` - 直接アクセスするURL（オプション）。
/// * `engine` - 検索エンジン（例: "google"）。`query`が指定された場合のみ有効。
///
/// # Returns
/// `Result<String, Box<dyn Error>>` - Markdown形式の検索結果、またはエラー。
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
        // URLが指定された場合、直接そのURLにアクセス
        fetch_url = target_url.to_string();
        action_description = format!("URLアクセス: '{}'", target_url);
    } else if let Some(search_query) = query {
        // クエリが指定された場合、Google検索を実行
        let used_engine = engine.unwrap_or("google");
        // Google検索URLを構築
        fetch_url = format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(search_query)
        );
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

    let html_content = client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("Webリクエストの送信に失敗しました: {}. URL: {}", e, fetch_url))?
        .error_for_status()? // HTTPステータスが2xx以外の場合はエラーを返す
        .text()
        .await
        .map_err(|e| format!("HTMLコンテンツの取得に失敗しました: {}. URL: {}", e, fetch_url))?;

    let final_markdown_content: String;

    if is_google_search {
        dprintln!(
            debug_mode,
            "[AI (ツール): Google検索結果のHTMLをパース中...]"
        );
        let search_results = google_search(client, debug_mode, query.unwrap()).await?; // 新しいgoogle_search関数を呼び出す

        let mut results_md = String::new();
        if search_results.is_empty() {
            results_md.push_str("検索結果が見つかりませんでした。\n");
        } else {
            results_md.push_str("### Google検索結果:\n");
            for (i, result) in search_results.iter().enumerate() {
                results_md.push_str(&format!(
                    "{}. [{}]({})\n",
                    i + 1,
                    result.title.trim(),
                    result.url
                ));
                if !result.description.is_empty() {
                    results_md.push_str(&format!("   {}\n", result.description.trim()));
                }
                results_md.push_str("\n");
            }
        }
        final_markdown_content = results_md;
    } else {
        // 通常のWebページアクセスの場合、ページ全体をMarkdownに変換
        dprintln!(
            debug_mode,
            "[AI (ツール): Webページ全体をMarkdownに変換中...]"
        );
        final_markdown_content = parse_html(&html_content);
    }

    // 最終的なMarkdownコンテンツが空または非常に短い場合のハンドリング
    let result_to_send = if final_markdown_content.trim().is_empty()
        || final_markdown_content.trim().len() < 50
    {
        dprintln!(
            debug_mode,
            "[AI (ツール): Markdown変換結果が空または短すぎます。元のHTMLの冒頭部分を簡潔に示します。]"
        );
        format!(
            "WebページのコンテンツをMarkdownに変換できませんでした。元のHTMLの冒頭部分:\n```html\n{}\n```\n",
            &html_content.chars().take(500).collect::<String>() // HTMLの冒頭500文字
        )
    } else {
        // トークン制限を考慮して結果を切り捨てる
        if final_markdown_content.len() > 4000 {
            format!(
                "{}\n...(結果は長すぎるため一部省略されました)",
                &final_markdown_content[..4000]
            )
        } else {
            final_markdown_content
        }
    };

    dprintln!(
        debug_mode,
        "[AI (ツール): 検索/アクセス結果のMarkdown変換が完了しました。]"
    );
    io::stdout().flush().unwrap_or_default();

    Ok(format!(
        "{}結果:\n```markdown\n{}\n```\n",
        action_description, result_to_send
    ))
}

/// Executes a Google search and returns a structured list of results.
/// This function directly scrapes Google search results, which can be fragile.
///
/// # Arguments
/// * `client` - The `reqwest::Client` instance to use for HTTP requests.
/// * `debug_mode` - A flag to enable or disable debug output.
/// * `search_text` - The query string for the Google search.
///
/// # Returns
/// `Result<Vec<SearchResult>, Box<dyn Error>>` - A vector of structured search results, or an error.
async fn google_search(
    client: &Client,
    debug_mode: bool,
    search_text: &str,
) -> Result<Vec<SearchResult>, Box<dyn Error>> {
    let search_url = format!(
        "https://www.google.com/search?q={}",
        urlencoding::encode(search_text)
    );
    dprintln!(debug_mode, "[DEBUG] Google検索URL: {}", search_url);

    let html_content = client.get(&search_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let document = Html::parse_document(&html_content);
    let mut results: Vec<SearchResult> = Vec::new();

    // 検索結果の主要なブロック
    let result_block_selector = Selector::parse("div.g").unwrap();

    for element in document.select(&result_block_selector) {
        let mut title = String::new();
        let mut url = String::new();
        let mut description = String::new();

        // タイトルセレクタの候補 (GoogleのHTMLは頻繁に変わるため、複数試す)
        let title_selectors = [
            Selector::parse("h3.LC20lb.MBeuO.DKV0Md").unwrap(), // 最も一般的
            Selector::parse("h3").unwrap(),                     // Fallback for any h3
            Selector::parse("div.s div div h3").unwrap(),       // Older common structure
        ];

        // リンクセレクタの候補
        let link_selectors = [
            Selector::parse("div.yuRUbf a").unwrap(), // Common link parent
            Selector::parse("a").unwrap(),            // General link within the block
        ];

        // スニペット/説明セレクタの候補
        let description_selectors = [
            Selector::parse("div.VwiC3b").unwrap(), // Common snippet class
            Selector::parse("div[data-sn-result]").unwrap(), // Alternative for snippets
            Selector::parse("span.aCOpRe").unwrap(), // Another snippet class
            Selector::parse("div.LGOjhe").unwrap(), // Older snippet container
        ];

        // タイトルの抽出
        for selector in &title_selectors {
            if let Some(node) = element.select(selector).next() {
                title = node.text().collect::<String>().trim().to_string();
                if !title.is_empty() {
                    break;
                }
            }
        }

        // URLの抽出
        for selector in &link_selectors {
            if let Some(node) = element.select(selector).next() {
                if let Some(href) = node.value().attr("href") {
                    url = href.to_string();
                    if !url.is_empty() {
                        break;
                    }
                }
            }
        }

        // 説明（スニペット）の抽出
        for selector in &description_selectors {
            if let Some(node) = element.select(selector).next() {
                description = node.text().collect::<String>().trim().to_string();
                if !description.is_empty() {
                    break;
                }
            }
        }

        // 少なくともタイトルとURLがあれば結果として追加
        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult {
                title,
                url,
                description,
            });
        }
    }

    dprintln!(
        debug_mode,
        "[DEBUG] Google検索で {} 件の構造化された結果を抽出しました。",
        results.len()
    );
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;
    use tokio; // tokioクレートのインポート // reqwest::Clientをインポート

    // 環境変数に依存するため、テストはネットワークアクセスを伴う統合テストになります
    // Googleに短時間で大量にリクエストを送るとブロックされる可能性があります
    #[tokio::test]
    async fn test_google_search_basic() -> Result<(), Box<dyn Error>> {
        let client = Client::new();
        let debug_mode = true; // テスト中はデバッグ出力を有効に

        dprintln!(debug_mode, "--- test_google_search_basic 開始 ---");

        let search_text = "Rust programming language";
        let results = google_search(&client, debug_mode, search_text).await?;

        // 結果が空でないことを確認
        assert!(!results.is_empty(), "検索結果が空です");

        // 最初の結果にタイトルとURLが含まれていることを確認
        if let Some(first_result) = results.first() {
            dprintln!(debug_mode, "最初の結果: {:?}", first_result);
            assert!(
                !first_result.title.is_empty(),
                "最初の結果のタイトルが空です"
            );
            assert!(!first_result.url.is_empty(), "最初の結果のURLが空です");
        } else {
            panic!("検索結果が空です。");
        }

        dprintln!(debug_mode, "--- test_google_search_basic 終了 ---");
        Ok(())
    }

    #[tokio::test]
    async fn test_google_search_specific_query() -> Result<(), Box<dyn Error>> {
        let client = Client::new();
        let debug_mode = true;

        dprintln!(debug_mode, "--- test_google_search_specific_query 開始 ---");

        let search_text = "香川県 丸亀城";
        let results = google_search(&client, debug_mode, search_text).await?;

        assert!(!results.is_empty(), "特定のクエリでの検索結果が空です");
        if let Some(first_result) = results.first() {
            dprintln!(debug_mode, "最初の結果 (香川県 丸亀城): {:?}", first_result);
            assert!(!first_result.title.is_empty(), "タイトルが空です");
            assert!(!first_result.url.is_empty(), "URLが空です");
            // 説明が空でも許容するが、存在すればチェック
            if !first_result.description.is_empty() {
                assert!(first_result.description.len() > 10, "説明が短すぎます");
            }
        }

        dprintln!(debug_mode, "--- test_google_search_specific_query 終了 ---");
        Ok(())
    }
}
