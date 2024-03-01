use balter::prelude::*;
use reqwest::Client;
use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<Client> = OnceLock::new();

use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    scenario_a()
        .tps(10_000)
        .duration(Duration::from_secs(120))
        .await;
}

#[scenario]
async fn scenario_a() {
    let _ = api_a().await;
}

#[transaction]
async fn api_a() -> Result<(), reqwest::Error> {
    let client = CLIENT.get_or_init(Client::new);
    client
        .get("http://0.0.0.0:3002/limited/7000/delay/ms/10/server/0")
        .send()
        .await?;
    Ok(())
}
