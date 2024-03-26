use futures::StreamExt;
use prep_instantiate::{get_route, Config, Result, Token};
use spend_limit::msg::{InstantiateMsg, TrackedDenom};
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> Result<()> {
    let conf: Config = toml::from_str(include_str!("../config.toml"))?;

    let concurrent_request_count = 10;
    let tracked_denoms = get_tracked_denom_infos(
        conf.tracked_denoms.clone(),
        conf.routing_amount_in
            .parse()
            .expect("Failed to parse routing amount in as u128"),
        &conf.price_resolution.quote_denom,
        concurrent_request_count,
    )
    .await;

    let msg = InstantiateMsg {
        price_resolution_config: conf.price_resolution,
        tracked_denoms,
    };

    // write instantiate msg to stdout
    let msg_str = serde_json::to_string(&msg)?;
    println!("{}", msg_str);

    Ok(())
}

async fn get_tracked_denom_infos(
    denoms: Vec<String>,
    routing_amount_in: u128,
    qoute_denom: &str,
    buffer_size: usize,
) -> Vec<TrackedDenom> {
    futures::stream::iter(denoms.into_iter().map(|denom| {
        let qoute_denom = qoute_denom.to_string();
        let handle: JoinHandle<TrackedDenom> = tokio::spawn(async move {
            let swap_routes = get_route(
                Token::new(routing_amount_in, denom.as_str()),
                qoute_denom.as_str(),
            )
            .await
            .expect("Failed to get route");

            TrackedDenom {
                denom: denom.to_string(),
                swap_routes,
            }
        });

        handle
    }))
    .buffer_unordered(buffer_size)
    .map(|handle| handle.expect("Failed to join handle"))
    .collect::<Vec<_>>()
    .await
}
