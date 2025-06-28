use std::collections::HashMap;

pub struct AIApi{
    info:HashMap<String,String>, // APIを使用するためのURLや認証情報を入れておく
}
impl AIApi{
    
}
pub enum ApiType{
    Ollama
}

// TODO OpenAIApi, GeminiAIApiなど、様々なサービスを使用して