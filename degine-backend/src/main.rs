use std::path::Path;

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./config.toml".to_string());
    let config = degine_backend::Config::load(Path::new(&path))
        .with_context(|| format!("failed to load config {path}"))?;
    degine_backend::serve(config).await
}
