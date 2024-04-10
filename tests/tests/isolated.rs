mod utils;
#[allow(unused)]
use utils::*;

#[cfg(feature = "integration")]
mod tests {
    use super::*;

    use balter::prelude::*;
    use reqwest::Client;
    use std::num::NonZeroU32;
    use std::sync::OnceLock;
    use std::time::Duration;

    #[tokio::test]
    async fn single_instance_tps() {
        init().await;

        let stats = scenario_1ms_delay()
            .tps(NonZeroU32::new(10_000).unwrap())
            //.duration(Duration::from_secs(30))
            .await;

        assert_eq!(stats.tps.get(), 10_000);
        assert!(stats.concurrency >= 10);
    }

    #[tokio::test]
    async fn single_instance_limited_tps() {
        init().await;

        let stats = scenario_1ms_limited_7000()
            .tps(NonZeroU32::new(10_000).unwrap())
            //.duration(Duration::from_secs(60))
            .await;

        assert!(dbg!(stats.tps.get()) <= 7_100);
        assert!(stats.concurrency >= 10);
    }

    #[tokio::test]
    async fn single_instance_error_rate() {
        init().await;

        tokio::time::sleep(Duration::from_secs(15)).await;

        let stats = scenario_1ms_max_2000()
            .saturate()
            //.duration(Duration::from_secs(60))
            .await;

        assert!(dbg!(stats.tps.get()) <= 2_300);
        assert!(dbg!(stats.tps.get()) >= 1_900);
        assert!(stats.concurrency >= 2);
    }

    /* Scenario Helpers */

    static CLIENT: OnceLock<Client> = OnceLock::new();

    #[scenario]
    async fn scenario_1ms_delay() {
        let _ = transaction_1ms().await;
    }

    #[transaction]
    async fn transaction_1ms() -> Result<(), reqwest::Error> {
        let client = CLIENT.get_or_init(Client::new);
        client.get("http://0.0.0.0:3002/delay/ms/1").send().await?;
        Ok(())
    }

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
            .get("http://0.0.0.0:3002/max/2000/delay/ms/1/scenario/0")
            .send()
            .await?;

        if res.status().is_server_error() {
            Err(anyhow::anyhow!("Err"))
        } else {
            Ok(())
        }
    }
}
