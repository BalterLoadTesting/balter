use balter::prelude::*;
use std::time::Duration;

use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    scenario_a()
        .tps(20_000)
        .duration(Duration::from_secs(120))
        .await;
}

#[scenario]
async fn scenario_a() {
    let _ = api_a().await;
}

#[transaction]
async fn api_a() -> Result<(), reqwest::Error> {
    reqwest::get("http://0.0.0.0:3000/api_10ms").await?;
    Ok(())
}
