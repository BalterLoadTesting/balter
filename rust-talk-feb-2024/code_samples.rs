#[transaction]
async fn create_post(client: &Client, data: &Data) -> Result<(), Error> {
    client.post("...")
        .json(data)
        .send()
        .await?;

    Ok(())
}


#[scenario]
async fn normal_user_load() {
    let user = sign_in().await;

    for _ in 0..10 {
        let _res = create_post()
            .await;
    }

    for _ in 0..50 {
        let _res = read_post()
            .await;
    }
}



async fn main() {

normal_user_load().await;

normal_user_load()
    .tps(10_000)
    .duration(Duration::from_secs(120))
    .await;

normal_user_load()
    .saturate()  // 3% Error Rate
    .duration(Duration::from_secs(120))
    .await;

normal_user_load()
    .overload()  // 80% Error Rate
    .duration(Duration::from_secs(120))
    .await;

normal_user_load()
    .error_rate(0.25)
    .duration(Duration::from_secs(120))
    .await;
}


