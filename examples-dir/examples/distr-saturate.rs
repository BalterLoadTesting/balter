use anyhow::{anyhow, Result};
use balter::prelude::*;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    BalterRuntime::new().with_args().run().await;
}

#[scenario]
async fn scenario_a() {
    let _ = api_a().await;
}

#[transaction]
async fn api_a() -> Result<()> {
    let res = reqwest::get("http://0.0.0.0:3000/api_max_tps").await?;
    if res.status().is_server_error() {
        Err(anyhow!("Server error"))
    } else {
        Ok(())
    }
}
