mod utils;
#[allow(unused)]
use utils::*;

#[cfg(feature = "integration")]
mod tests {
    use super::*;
    use balter::prelude::*;
    use mock_service::prelude::*;
    use reqwest::Client;
    use std::num::NonZeroU32;
    use std::sync::OnceLock;
    use std::time::Duration;
    use tracing::debug;

    #[tokio::test]
    async fn single_instance_latency() {
        init().await;

        let stats = latency_200ms_scenario()
            .latency(Duration::from_millis(200), 0.9)
            //.duration(Duration::from_secs(60))
            .await;

        assert!(dbg!(stats.tps.get()) <= 2_300);
        assert!(dbg!(stats.tps.get()) >= 1_900);
        assert!(stats.concurrency >= 2);
    }

    static CLIENT: OnceLock<Client> = OnceLock::new();

    #[scenario]
    async fn latency_200ms_scenario() {
        let _ = latency_200ms_call().await;
    }

    #[transaction]
    async fn latency_200ms_call() -> Result<(), anyhow::Error> {
        let client = CLIENT.get_or_init(Client::new);
        let res = client
            .get("http://0.0.0.0:3002/")
            .json(&Config {
                scenario_name: "latency_isolated".to_string(),
                tps: None,
                latency: Some(LatencyConfig {
                    latency: Duration::from_millis(200),
                    kind: LatencyKind::Linear(NonZeroU32::new(2000).unwrap()),
                }),
            })
            .send()
            .await?;

        if res.status().is_server_error() {
            Err(anyhow::anyhow!("Err"))
        } else {
            Ok(())
        }
    }
}
