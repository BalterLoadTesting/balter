use anyhow::{anyhow, Result};
use balter::prelude::*;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    scenario_a()
        .saturate()
        .duration(Duration::from_secs(120))
        .await;
}

#[scenario]
async fn scenario_a() {
    let _ = api_a().await;
}

#[transaction]
async fn api_a() -> Result<()> {
    let res = reqwest::get("http://0.0.0.0:3002/max/2000/delay/ms/10/scenario/0").await?;
    if res.status().is_server_error() {
        Err(anyhow!("Server error"))
    } else {
        Ok(())
    }
}
