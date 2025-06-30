use crate::modules::cli::App; // Import the App struct

mod modules; // Make sure this line exists if not already

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let ollama_base_url = "http://localhost:11434".to_string();
    let default_ollama_model = "gemma3:latest".to_string(); // Adjust model name as needed

    // Create a new App instance and run it
    let mut app = App::new(ollama_base_url, default_ollama_model);
    app.run().await
}