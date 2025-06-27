// src/modules/chat/api/search.rs

use reqwest::Client;
use std::error::Error;
use std::io::{self, Write};
use html2md::parse_html;
use urlencoding; // urlencoding クレートをインポート
use crate::dprintln; // src/lib.rs または src/main.rs で定義されたマクロをインポート

/// 実際のWeb検索またはURLアクセスを実行し、HTMLをMarkdownにパースして返す
///
/// # 引数
/// * `client` - HTTPリクエストに使用するreqwest::Clientインスタンス。
/// * `debug_mode` - デバッグ出力を有効にするかどうか。
/// * `query` - Google検索に使用するクエリ（オプション）。
/// * `url` - 直接アクセスするURL（オプション）。
/// * `engine` - 検索エンジン（例: "google"）。`query`が指定された場合のみ有効。
///
/// # 戻り値
/// `Result<String, Box<dyn Error>>` - Markdown形式の検索結果、またはエラー。
pub async fn execute_web_search(
    client: &Client, // Clientを引数として受け取る
    debug_mode: bool, // デバッグモードのフラグを引数として受け取る
    query: Option<&str>,
    url: Option<&str>,
    engine: Option<&str>,
) -> Result<String, Box<dyn Error>> {
    let fetch_url;
    let action_description: String;

    if let Some(target_url) = url {
        // URLが指定された場合、直接そのURLにアクセス
        fetch_url = target_url.to_string();
        action_description = format!("URLアクセス: '{}'", target_url);
    } else if let Some(search_query) = query {
        // クエリが指定された場合、Google検索を実行
        let used_engine = engine.unwrap_or("google");
        // Google検索URLを構築
        fetch_url = format!("https://www.google.com/search?q={}", urlencoding::encode(search_query));
        action_description = format!("{}検索: '{}'", used_engine, search_query);
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

    // HTMLをMarkdownに変換
    let markdown_content = parse_html(&html_content);

    // トークン制限を考慮して結果を切り捨てる
    let truncated_markdown = if markdown_content.len() > 4000 {
        // 適切な長さに調整
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
