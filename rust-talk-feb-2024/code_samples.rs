
#[tokio::main]
async fn main() {
    my_scenario()
        .tps(10_000)
        .duration(Duration::from_secs(120))
        .await;
}

#[scenario]
async fn my_scenario() {
    let _ = my_transaction().await;
}

#[transaction]
async fn my_transaction() -> Result<()> {
    some_remote_call().await
}



#[transaction]
async fn create_post(client: &Client, data: &Data) -> Result<(), Error> {
    let res = client.post("...")
        .json(data)
        .send()
        .await?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(...)
    }
}


#[scenario]
async fn normal_user_load() {
    let user = sign_in().await;

    for _ in 0..10 {
        let _res = create_post().await;
    }

    for _ in 0..50 {
        let _res = read_post().await;
    }
}

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


normal_user_load()
    .direct(200, Some(10_000))
    .duration(Duration::from_secs(120))
    .await;




join!(
    scenario_a().saturate().duration(from_secs(300)),
    async {
        sleep(30).await;
        scenario_b().tps(10_000).duration(from_secs(30)).await
    }
)

join!(
    scenario_a().saturate().duration(from_secs(300)),
    async {
        sleep(30).await;
        shutdown_half().await;
    }
)




#[tokio::main]
async fn main() {
	BalterRuntime::new()
		.with_autoscaling_hook(balter_aws::autoscaling_hook)
		.with_mtls(params)
		.with_args()
		.run()
		.await;
}



#[scenario]
async fn scenario_a(i32_fuzz: Fuzz<i32>) {
	let res = my_transaction(i32_fuzz).await;
}

#[transaction]
transaction_a(arg: i32) -> Result<i32> {
	some_remote_call(arg).await
}

// Or

#[transaction(fuzz(arg))]
transaction_a(arg: i32) -> Result<i32> {
	some_remote_call(arg).await
}




scenario_a()
	.tps(10_000)
	.duration(from_secs(300))
	.await;

#[scenario]
async fn scenario_a() {
	...
}


fn scenario_a() -> Scenario<impl Fn() -> impl Future> {
	Scenario {
		func: __balter_scenario_a,
		...
	}
}

async fn __balter_scenario_a() {
	...
}



/// Scenario

tokio::task_local! {
	static TRANSACTION_HOOK: TransactionData;
}

tokio::spawn(TRANSACTION_HOOK.scope(
	transaction_data,
	async move {
		loop {
			scenario().await;
		}
	}
));


/// Transaction

if let Ok(hook) = TRANSACTION_HOOK.try_with(|v| v.clone()) {
	...
}
