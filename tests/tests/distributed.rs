#[cfg(feature = "integration")]
mod tests {

    use balter::prelude::*;
    use reqwest::Client;
    use std::net::SocketAddr;
    use std::sync::OnceLock;
    use std::time::Duration;
    use tracing_subscriber::FmtSubscriber;

    pub async fn init() {
        static ONCE_LOCK: OnceLock<()> = OnceLock::new();

        let wait = ONCE_LOCK.get().is_none();

        ONCE_LOCK.get_or_init(|| {
            FmtSubscriber::builder()
                .with_env_filter("balter=debug,axum::rejection=trace")
                .init();

            tokio::spawn(async {
                let addr: SocketAddr = "0.0.0.0:3002".parse().unwrap();
                mock_service::run(addr).await;
            });

            tokio::spawn(async {
                BalterRuntime::new().port(7621).run().await;
            });
        });

        if wait {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    #[tokio::test]
    async fn single_instance_rt() {
        init().await;

        let client = Client::new();
        let res = client
            .post("http://0.0.0.0:7621/run")
            .json(&serde_json::json!({
                "name": "scenario_1ms_delay",
                "duration": 120,
                "kind": {
                    "Tps": 300,
                }
            }))
            .send()
            .await
            .expect("Request failed");

        assert!(res.status().is_success());
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
}
