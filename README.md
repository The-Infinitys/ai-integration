# ai-integration

`ai-integration` は、AuraScript と呼ばれるシンプルなスクリプト言語のプロトタイプです。このプロジェクトは、ファイル操作、変数管理、そして OpenAI、Ollama、Google Gemini といった主要な AI プロバイダーとの連携を可能にすることで、AI アシスタントの自動化タスクを記述する体験を提供します。

## 特徴

- **シンプルなスクリプト言語**: `let` と `Print` コマンドで、直感的かつ簡単にスクリプトを作成できます。
- **ファイル操作**: `Read file` コマンドでファイルの内容を読み込むことができます。
- **複数の AI プロバイダー対応**:
  - **OpenAI**: GPT モデルとの連携。
  - **Ollama**: ローカルで実行中の Ollama モデルとの連携。
  - **Google Gemini**: Gemini API との連携。
- **インタラクティブチャットモード**: 指定した AI プロバイダーと直接チャットできるコンソールベースのインターフェース。
- **設定管理**: AI プロバイダーの API キーやモデル名などを一元管理。

## 始め方

### 前提条件

- Rust プログラミング言語がインストールされていること。
  - [Rust 公式サイト](https://www.rust-lang.org/tools/install) からインストールできます。
- AI プロバイダーを使用する場合、それぞれの API キーまたはローカル環境の設定が必要です。
  - **OpenAI**: [OpenAI API キー](https://platform.openai.com/account/api-keys)
  - **Ollama**: [Ollama 公式サイト](https://ollama.com/) からインストールし、モデルをダウンロードしてください。
  - **Google Gemini**: [Google Cloud Console](https://console.cloud.google.com/apis/credentials) で Gemini API を有効にし、API キーを取得してください。

### インストール

1.  このリポジトリをクローンします。

    ```bash
    git clone https://github.com/your-username/ai-integration.git
    cd ai-integration
    ```

2.  プロジェクトの依存関係をビルドします。

    ```bash
    cargo build
    ```

### 設定

`src/config.rs` ファイルを開き、`Config::new()` メソッド内のプレースホルダーを実際の API キーや設定に更新してください。

```rust
// src/config.rs

pub struct Config {
    pub openai_api_key: String,
    pub openai_model: String,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub gemini_api_key: String,
    pub gemini_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
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

```

**重要**: セキュリティ上の理由から、**本番環境では API キーをコードに直接埋め込むべきではありません**。環境変数や安全な設定管理ツールを使用することを強く推奨します。

## 使用方法

### デフォルトスクリプトの実行

引数なしで `cargo run` を実行すると、プロジェクトに組み込まれているデフォルトの AuraScript が実行されます。このスクリプトには、変数代入、ファイル読み込み、および各 AI プロバイダーからのコンテンツ生成の例が含まれています。

```bash
cargo run
```

### インタラクティブチャットモード

特定の AI プロバイダーと直接チャットしたい場合は、`prompt` コマンドを使用します。

```bash
cargo run -- prompt [provider]
```

例:

- OpenAI とチャット:
  ```bash
  cargo run -- prompt openai
  ```
- Ollama とチャット:
  ```bash
  cargo run -- prompt ollama
  ```
- Google Gemini とチャット:
  ```bash
  cargo run -- prompt gemini
  ```

チャット中に `'exit'` または `'quit'` と入力すると、チャットセッションが終了します。

### ヘルプの表示

利用可能なコマンドや使用方法について確認するには、`help` コマンドを使用します。

```bash
cargo run -- help
```

## AuraScript の構文 (プロトタイプ)

現在サポートされている AuraScript の構文は以下の通りです。

- **変数宣言と代入**:
  ```aurascript
  let my_variable = "Hello, World!";
  let file_content = Read file "path/to/my_file.txt";
  let ai_response = Generate content from "openai" with prompt "What is Rust?";
  ```
- **コンソール出力**:
  ```aurascript
  Print my_variable;
  Print "Direct string output";
  ```
- **コメント**:
  `//` で始まる行はコメントとして無視されます。

## 開発ロードマップ (例)

- より高度なデータ型 (数値、ブーリアン、リストなど) のサポート。
- 条件分岐 (`if/else`) とループ (`for/while`) の実装。
- カスタム関数の定義と呼び出し。
- モジュールシステムによるスクリプトの分割。
- エラーハンドリングの強化と詳細なエラーメッセージ。
- 外部スクリプトファイルの読み込みと実行。
- CLI 引数によるスクリプトパスの指定。

---

この `ai-integration` プロジェクトは、AI との連携をスクリプトで自動化するための可能性を示すものです。ぜひご自身の環境で試してみてくださいね。何か質問やフィードバックがあれば、お気軽にお寄せください！
