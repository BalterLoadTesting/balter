use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{error, Level};
use tracing_subscriber::FmtSubscriber;

#[allow(unused)]
pub async fn init() {
    static ONCE_LOCK: OnceLock<()> = OnceLock::new();

    let wait = ONCE_LOCK.get().is_none();

    ONCE_LOCK.get_or_init(|| {
        let default_panic = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            default_panic(info);
            error!("Panic occurred: {info:?}");
            std::process::exit(1);
        }));

        FmtSubscriber::builder()
            .with_max_level(Level::DEBUG)
            .with_env_filter("balter=trace,mock_service=debug,axum::rejection=trace")
            .init();

        PrometheusBuilder::new()
            .with_http_listener("0.0.0.0:8002".parse::<SocketAddr>().unwrap())
            .install()
            .unwrap();

        tokio::spawn(async {
            let addr: SocketAddr = "0.0.0.0:3002".parse().unwrap();
            mock_service::run(addr).await;
        });
    });

    if wait {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
