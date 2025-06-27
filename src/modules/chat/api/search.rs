// src/modules/chat/api/search.rs

use reqwest::Client;
use std::error::Error;
use std::io::{self, Write};
use html2md::parse_html;
use urlencoding;
use crate::dprintln;
use scraper::{Html, Selector}; // scraperはgoogle_searchの構造化パースに引き続き使用
use crate::utils::xml_editor; // 自作のXMLエディターをインポート

/// Cleans the HTML content by removing script, style, and other common unwanted tags/elements.
/// Uses `xml_editor` for robust DOM manipulation.
fn clean_html(html_content: &str) -> String {
    let unwanted_tags_for_stripping = [
        "script",
        "style",
        "header",
        "footer",
        "nav",
        "aside",
        "form",
        "noscript",
        "iframe",
        // "img",      // 画像を削除するかは用途によるが、デフォルトでは残す。必要なら追加。
        // "svg",      // SVGを削除するかは用途によるが、デフォルトでは残す。必要なら追加。
        "link", // linkタグ全体を削除 (CSS, faviconなど)
        "meta",
        "input",
        "button",
        // よく見られる余計な要素のセレクタをタグ名として追加（クラスやIDではない）
        "div", // divは非常に一般的だが、特定のクラスやIDがない場合は大量に削除される可能性があるため注意
        "span", // spanも同様
        "a", // リンクも削除される可能性があるため注意
    ];

    // 自作のxml_editorを使ってHTML文字列をクリーンアップ
    let cleaned_html_str = xml_editor::clean_html_string(html_content, &unwanted_tags_for_stripping);
    
    // clean_html_stringはHTML構造を簡素化するため、その結果をhtml2mdに渡す
    cleaned_html_str
}


/// Executes a real web search or directly accesses a URL, parses the HTML to Markdown, and returns it.
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
        fetch_url = target_url.to_string();
        action_description = format!("URLアクセス: '{}'", target_url);
    } else if let Some(search_query) = query {
        let used_engine = engine.unwrap_or("google");
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

    let html_content = client.get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("Webリクエストの送信に失敗しました: {}. URL: {}", e, fetch_url))?
        .error_for_status()?
        .text()
        .await
        .map_err(|e| format!("HTMLコンテンツの取得に失敗しました: {}. URL: {}", e, fetch_url))?;

    let final_markdown_content: String;

    if is_google_search {
        dprintln!(debug_mode, "[AI (ツール): Google検索結果のHTMLをパース中...]");
        final_markdown_content = google_search(client, debug_mode, query.unwrap()).await?;

    } else {
        dprintln!(debug_mode, "[AI (ツール): Webページ全体をクリーンアップしてMarkdownに変換中...]");
        let cleaned_html = clean_html(&html_content); // 自作ライブラリを使用
        final_markdown_content = parse_html(&cleaned_html); // cleaned_htmlはStringなのでそのまま渡す
    }

    let result_to_send = if final_markdown_content.trim().is_empty() || final_markdown_content.trim().len() < 50 {
        dprintln!(debug_mode, "[AI (ツール): Markdown変換結果が空または短すぎます。元のHTMLの冒頭部分を簡潔に示します。]");
        format!(
            "WebページのコンテンツをMarkdownに変換できませんでした。元のHTMLの冒頭部分:\n```html\n{}\n```\n",
            &html_content.chars().take(500).collect::<String>()
        )
    } else {
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


/// Executes a Google search and returns the results directly as a Markdown string.
/// This function directly scrapes Google search results, which can be fragile.
///
/// # Arguments
/// * `client` - The `reqwest::Client` instance to use for HTTP requests.
/// * `debug_mode` - A flag to enable or disable debug output.
/// * `search_text` - The query string for the Google search.
///
/// # Returns
/// `Result<String, Box<dyn Error>>` - A Markdown formatted string of search results, or an error.
async fn google_search(client: &Client, debug_mode: bool, search_text: &str) -> Result<String, Box<dyn Error>> {
    let search_url = format!("https://www.google.com/search?q={}", urlencoding::encode(search_text));
    dprintln!(debug_mode, "[DEBUG] Google検索URL: {}", search_url);

    let html_content = client.get(&search_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let document = Html::parse_document(&html_content); // scraperでパース
    let mut results_md = String::new();
    
    let result_block_selector = Selector::parse("div.g").unwrap();

    let mut found_any_results = false;

    for element in document.select(&result_block_selector) {
        let mut title = String::new();
        let mut url = String::new();
        let mut description = String::new();

        let title_selectors = [
            Selector::parse("h3.LC20lb.MBeuO.DKV0Md").unwrap(),
            Selector::parse("h3").unwrap(),
            Selector::parse("a > h3").unwrap(),
            Selector::parse("div.r > a").unwrap(),
        ];

        let link_selectors = [
            Selector::parse("div.yuRUbf a").unwrap(),
            Selector::parse("a[href]").unwrap(),
        ];

        let description_selectors = [
            Selector::parse("div.VwiC3b").unwrap(),
            Selector::parse("div[data-sn-result]").unwrap(),
            Selector::parse("span.aCOpRe").unwrap(),
            Selector::parse("div.LGOjhe").unwrap(),
            Selector::parse("div.g-blk.g-sm div.st").unwrap(),
        ];

        for selector in &title_selectors {
            if let Some(node) = element.select(selector).next() {
                title = node.text().collect::<String>().trim().to_string();
                if !title.is_empty() { break; }
            }
        }

        for selector in &link_selectors {
            if let Some(node) = element.select(selector).next() {
                if let Some(href) = node.value().attr("href") {
                    if href.starts_with("/url?q=") {
                        if let Some(decoded_url) = urlencoding::decode(href.trim_start_matches("/url?q=")).ok() {
                            if let Some(end_idx) = decoded_url.find('&') {
                                url = decoded_url[..end_idx].to_string();
                            } else {
                                url = decoded_url.to_string();
                            }
                        }
                    } else {
                        url = href.to_string();
                    }
                    if !url.is_empty() { break; }
                }
            }
        }

        for selector in &description_selectors {
            if let Some(node) = element.select(selector).next() {
                description = node.text().collect::<String>().trim().to_string();
                if !description.is_empty() { break; }
            }
        }
        
        if !title.is_empty() && !url.is_empty() {
            results_md.push_str(&format!("* **[{}]({})**\n", title, url));
            if !description.is_empty() {
                results_md.push_str(&format!("  {}\n", description));
            }
            results_md.push_str("\n");
            found_any_results = true;
        }
    }

    if !found_any_results {
        dprintln!(debug_mode, "[AI (ツール): 構造化されたGoogle検索結果が見つかりませんでした。HTMLをクリーンアップ後、全ページをMarkdownに変換します。]");
        let cleaned_html_for_fallback = clean_html(&html_content); // ここでclean_htmlを呼び出し
        results_md = parse_html(&cleaned_html_for_fallback);
    } else {
        dprintln!(debug_mode, "[DEBUG] Google検索で構造化された結果をMarkdownに変換しました。");
    }

    Ok(results_md)
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    use reqwest::Client;

    #[tokio::test]
    async fn test_clean_html_removes_scripts_styles_and_more_my_html_editor() -> Result<(), Box<dyn Error>> {
        let html_input = r#"
            <html>
            <head>
                <style>body { color: red; }</style>
                <script>console.log('hello');</script>
                <meta charset="utf-8">
                <link rel="stylesheet" href="style.css">
            </head>
            <body>
                <header><h1>Header Title</h1></header>
                <nav><ul><li><a href=" #">Nav Link</a></li></ul></nav>
                <form action="submit"><input type="text" name="q"></form>
                <main>
                    <h1>Main Title</h1>
                    <p>Some text.<script>alert('evil');</script></p>
                    <aside>Sidebar content</aside>
                </main>
                <footer>&copy; 2023</footer>
                <noscript><p>Please enable JS</p></noscript>
                <iframe></iframe>
                <img>
                <svg></svg>
                <div class="ads">Ad content</div>
                <div id="sidebar">Sidebar content</div>
                <div class="cookie-banner">Cookie notice</div>
                <div id="comments">Comments section</div>
            </body>
            </html>
        "#;

        let cleaned_html = clean_html(html_input);
        
        // Check that unwanted tags/content are gone
        assert!(!cleaned_html.contains("<script"), "Script tag was not removed.");
        assert!(!cleaned_html.contains("console.log('hello');"), "Script content was not removed.");
        assert!(!cleaned_html.contains("alert('evil');"), "Inline script content was not removed.");
        assert!(!cleaned_html.contains("<style"), "Style tag was not removed.");
        assert!(!cleaned_html.contains("body { color: red; }"), "Style content was not removed.");
        assert!(!cleaned_html.contains("<header"), "Header tag was not removed.");
        assert!(!cleaned_html.contains("Header Title"), "Header content was not removed.");
        assert!(!cleaned_html.contains("<nav"), "Nav tag was not removed.");
        assert!(!cleaned_html.contains("Nav Link"), "Nav content was not removed.");
        assert!(!cleaned_html.contains("<form"), "Form tag was not removed.");
        assert!(!cleaned_html.contains("<input"), "Input tag was not removed.");
        assert!(!cleaned_html.contains("<aside"), "Aside tag was not removed.");
        assert!(!cleaned_html.contains("Sidebar content"), "Aside content was not removed."); 
        assert!(!cleaned_html.contains("<footer"), "Footer tag was not removed.");
        assert!(!cleaned_html.contains("&copy;"), "Footer content was not removed.");
        assert!(!cleaned_html.contains("<noscript"), "Noscript tag was not removed.");
        assert!(!cleaned_html.contains("Please enable JS"), "Noscript content was not removed.");
        assert!(!cleaned_html.contains("<iframe"), "Iframe tag was not removed.");
        assert!(!cleaned_html.contains("<img"), "Img tag was not removed.");
        assert!(!cleaned_html.contains("<svg"), "Svg tag was not removed.");
        assert!(!cleaned_html.contains("<link"), "Link tag was not removed.");
        assert!(!cleaned_html.contains("<meta"), "Meta tag was not removed.");
        assert!(!cleaned_html.contains("Ad content"), "Ad content was not removed.");
        assert!(!cleaned_html.contains("Sidebar content"), "Sidebar content (from #sidebar) was not removed.");
        assert!(!cleaned_html.contains("Cookie notice"), "Cookie notice was not removed.");
        assert!(!cleaned_html.contains("Comments section"), "Comments section was not removed.");

        assert!(cleaned_html.contains("<h1>Main Title</h1>"), "Main title was removed unexpectedly.");
        assert!(cleaned_html.contains("<p>Some text.</p>"), "Paragraph was not removed unexpectedly.");

        dprintln!(true, "Cleaned HTML:\n{}", cleaned_html);
        Ok(())
    }

    #[tokio::test]
    async fn test_google_search_returns_markdown() -> Result<(), Box<dyn Error>> {
        let client = Client::new();
        let debug_mode = true;

        dprintln!(debug_mode, "--- test_google_search_returns_markdown 開始 ---");

        let search_text = "Rust async await";
        let markdown_output = google_search(&client, debug_mode, search_text).await?;

        assert!(!markdown_output.is_empty(), "Google検索のMarkdown出力が空です");
        
        assert!(markdown_output.contains("["), "Markdown出力にリンクの開始カッコがありません");
        assert!(markdown_output.contains("]"), "Markdown出力にリンクの終了カッコがありません");
        assert!(markdown_output.contains("("), "Markdown出力にURLの開始カッコがありません");
        assert!(markdown_output.contains(")"), "Markdown出力にURLの終了カッコがありません");
        assert!(markdown_output.contains("**"), "Markdown出力に太字のマークダウンがありません");

        dprintln!(debug_mode, "google_search Markdown Output:\n{}", markdown_output);
        dprintln!(debug_mode, "--- test_google_search_returns_markdown 終了 ---");
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_web_search_with_url_cleaning() -> Result<(), Box<dyn Error>> {
        let client = Client::new();
        let debug_mode = true;

        dprintln!(debug_mode, "--- test_execute_web_search_with_url_cleaning 開始 ---");

        let test_url = "https://www.rust-lang.org/"; 
        let output_md = execute_web_search(&client, debug_mode, None, Some(test_url), None).await?;

        dprintln!(debug_mode, "URL Access Markdown Output:\n{}", output_md);

        assert!(!output_md.is_empty(), "URLアクセスからのMarkdown出力が空です");
        assert!(!output_md.contains("<script"), "出力Markdownにscriptタグが含まれています (クリーンアップの問題)");
        assert!(!output_md.contains("<style"), "出力Markdownにstyleタグが含まれています (クリーンアップの問題)");

        assert!(output_md.contains("Rust"), "Markdown出力に 'Rust' が含まれていません");

        dprintln!(debug_mode, "--- test_execute_web_search_with_url_cleaning 終了 ---");
        Ok(())
    }
}
