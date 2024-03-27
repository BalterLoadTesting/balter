use balter::prelude::*;
use std::num::NonZeroU32;
use std::time::Duration;
use tracing::info;

use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    BalterRuntime::new().with_args().run().await;
}

#[scenario]
async fn root_scenario() {
    tokio::join!(
        scenario_a()
            .tps(NonZeroU32::new(500).unwrap())
            .duration(Duration::from_secs(30)),
        scenario_b()
            .tps(NonZeroU32::new(1000).unwrap())
            .duration(Duration::from_secs(30)),
    );

    info!("Complete?");
}

#[scenario]
async fn scenario_a() {
    let _ = api_a().await;
}

#[scenario]
async fn scenario_b() {
    let _ = api_b().await;
}

#[transaction]
async fn api_a() -> Result<(), reqwest::Error> {
    reqwest::get("http://0.0.0.0:3000/api_10ms").await?;
    Ok(())
}

#[transaction]
async fn api_b() -> Result<(), reqwest::Error> {
    reqwest::get("http://0.0.0.0:3000/api_10ms").await?;
    Ok(())
}
