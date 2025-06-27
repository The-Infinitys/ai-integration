// src/config.rs

pub struct Config {
    pub openai_api_key: String,
    pub openai_model: String,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub gemini_api_key: String,
    pub gemini_model: String,
}

impl Config {
    pub fn new() -> Self {
        // !!! 警告: これはデモンストレーション目的であり、APIキーをハードコードすることは非推奨です。
        // 実際のアプリケーションでは、環境変数、シークレットマネージャー、または安全な設定ファイルから読み込むべきです。
        println!("警告: APIキーはハードコードされています。本番環境では使用しないでください！");

        Config {
            openai_api_key: "YOUR_OPENAI_API_KEY".to_string(), // ★ここに実際のOpenAI APIキーを設定してください
            openai_model: "gpt-3.5-turbo".to_string(),
            ollama_base_url: "http://localhost:11434/api/generate".to_string(),
            ollama_model: "llama2".to_string(), // ★Ollamaでダウンロード済みのモデルを設定してください
            gemini_api_key: "YOUR_GEMINI_API_KEY".to_string(), // ★ここに実際のGemini APIキーを設定してください
            gemini_model: "gemini-pro".to_string(),
        }
    }
}
