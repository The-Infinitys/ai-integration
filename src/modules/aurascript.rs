/// AIが使用するコマンドラインを設定する
///
/// ## 例
///
/// ```bash
/// !ls                     # ターミナルのコマンドを実行する
/// /web_search Rust Google # 予め使用できるコマンドを設定しておき、AIが利用できるようにする
/// ```
///
///
pub struct AuraScriptRunner {
    scripts: Vec<String>,
}
