mod utils;
#[allow(unused)]
use utils::*;

#[cfg(feature = "integration")]
mod tests {
    use super::*;
    use balter::prelude::*;
    use mock_service::prelude::*;
    use reqwest::Client;
    use std::sync::OnceLock;
    use std::time::Duration;

    #[tokio::test]
    async fn transparent_scenario_call() {
        let _ = scenario_1ms_delay().await;
    }

    #[tokio::test]
    async fn single_instance_tps() {
        init().await;

        let stats = scenario_1ms_delay()
            .tps(10_000)
            .duration(Duration::from_secs(30))
            .await;

        assert_eq!(stats.goal_tps, 10_000);
        assert!(stats.actual_tps > 9_500.);
        assert!(stats.concurrency >= 10);
    }

    #[scenario]
    async fn scenario_1ms_delay() {
        let client = Client::new();
        loop {
            let _ = transaction_1ms(&client).await;
        }
    }

    #[transaction]
    async fn transaction_1ms(client: &Client) -> Result<(), reqwest::Error> {
        let _res = client
            .get("http://0.0.0.0:3002/")
            .json(&Config {
                scenario_name: "tps_isolated".to_string(),
                tps: None,
                latency: Some(LatencyConfig {
                    latency: Duration::from_millis(1),
                    kind: LatencyKind::Delay,
                }),
            })
            .send()
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn single_instance_noisy_tps() {
        init().await;

        let stats = scenario_1ms_noisy_delay()
            .tps(500)
            .duration(Duration::from_secs(80))
            .await;

        assert_eq!(stats.goal_tps, 500);
        assert!(stats.actual_tps > 480.);
        assert!(stats.concurrency >= 10);
    }

    #[scenario]
    async fn scenario_1ms_noisy_delay() {
        let client = Client::new();
        loop {
            let _ = transaction_noisy_1ms(&client).await;
        }
    }

    #[transaction]
    async fn transaction_noisy_1ms(client: &Client) -> Result<(), reqwest::Error> {
        let _res = client
            .get("http://0.0.0.0:3002/")
            .json(&Config {
                scenario_name: "tps_isolated".to_string(),
                tps: None,
                latency: Some(LatencyConfig {
                    latency: Duration::from_millis(400),
                    kind: LatencyKind::Noise(Duration::from_millis(300), 50.),
                }),
            })
            .send()
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn single_instance_limited_tps() {
        init().await;

        let stats = scenario_1ms_limited_7000()
            .tps(10_000)
            .duration(Duration::from_secs(30))
            .await;

        assert!(dbg!(stats.goal_tps) <= 7_100);
        assert!(dbg!(stats.actual_tps) > 6_500.);
        assert!(stats.concurrency >= 10);
    }

    #[tokio::test]
    async fn single_instance_error_rate() {
        init().await;

        let stats = scenario_1ms_max_2000()
            .error_rate(0.03)
            .duration(Duration::from_secs(60))
            .await;

        assert!(dbg!(stats.error_rate) > 0.0);
        assert!(dbg!(stats.error_rate) < 0.07);
        assert!(dbg!(stats.goal_tps) <= 1_400);
        assert!(dbg!(stats.goal_tps) >= 1_200);
        assert!(stats.concurrency >= 2);
    }

    /* Scenario Helpers */

    static CLIENT: OnceLock<Client> = OnceLock::new();

    #[scenario]
    async fn scenario_1ms_limited_7000() {
        let _ = transaction_1ms_limited_7000().await;
    }

    #[transaction]
    async fn transaction_1ms_limited_7000() -> Result<(), reqwest::Error> {
        let client = CLIENT.get_or_init(Client::new);
        client
            .get("http://0.0.0.0:3002/limited/7000/delay/ms/1/server/0")
            .send()
            .await?;
        Ok(())
    }

    #[scenario]
    async fn scenario_1ms_max_2000() {
        let _ = transaction_1ms_max_2000().await;
    }

    #[transaction]
    async fn transaction_1ms_max_2000() -> anyhow::Result<()> {
        let client = CLIENT.get_or_init(Client::new);
        let res = client
            .get("http://0.0.0.0:3002/max/1300/delay/ms/1/scenario/0")
            .send()
            .await?;

        if res.status().is_server_error() {
            Err(anyhow::anyhow!("Err"))
        } else {
            Ok(())
        }
    }
}
