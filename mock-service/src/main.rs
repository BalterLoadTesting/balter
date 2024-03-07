#[allow(unused)]
use metrics::{counter, gauge, histogram};
//use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    /*
    PrometheusBuilder::new()
        .with_http_listener("0.0.0.0:3001".parse::<SocketAddr>().unwrap())
        .install()
        .unwrap();
        */

    tokio::task::spawn(async { mock_service::tps_measure_task().await });

    let addr: SocketAddr = "0.0.0.0:3002".parse().unwrap();
    mock_service::run(addr).await;
}
