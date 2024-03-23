use futures::StreamExt;
use std::fmt::Display;
use tokio::task::JoinHandle;

use error_chain::error_chain;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
        Json(serde_json::Error);
        Toml(toml::de::Error);
        JoinError(tokio::task::JoinError);
    }
}

use serde::{Deserialize, Serialize};
use spend_limit::{
    msg::{InstantiateMsg, SwapAmountInRoute, TrackedDenom},
    price::PriceResolutionConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    let conf: Config = toml::from_str(include_str!("../config.toml"))?;

    let tracked_denoms =
        futures::stream::iter(conf.tracked_denoms.clone().into_iter().map(|denom| {
            // TODO: handle error > unwrap
            let conf = conf.clone();
            let handle: JoinHandle<TrackedDenom> = tokio::spawn(async move {
                let amount = conf.routing_amount_in.parse().unwrap();
                let swap_routes = get_route(
                    Token::new(amount, denom.as_str()),
                    &conf.price_resolution.quote_denom,
                )
                .await
                .unwrap();

                TrackedDenom {
                    denom: denom.to_string(),
                    swap_routes,
                }
            });

            handle
        }))
        .buffer_unordered(10)
        .map(|handle| handle.unwrap()) // TODO: handle error > unwrap
        .collect::<Vec<_>>()
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    /// The price resolution config used directly in the instantiate msg
    price_resolution: PriceResolutionConfig,

    /// The amount of token to calculate route via sqs
    routing_amount_in: String,

    /// The denoms to track, used for calculating route via sqs
    tracked_denoms: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RouterResponse {
    amount_in: Token,
    amount_out: String,
    route: Vec<Route>,
    effective_fee: String,
    price_impact: String,
    in_base_out_quote_spot_price: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Token {
    denom: String,
    amount: String,
}

impl Token {
    fn new(amount: u128, denom: &str) -> Self {
        Self {
            amount: amount.to_string(),
            denom: denom.to_string(),
        }
    }
}
impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.amount, self.denom)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Route {
    pools: Vec<Pool>,
    #[serde(rename = "has-cw-pool")]
    has_cw_pool: bool,
    out_amount: String,
    in_amount: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pool {
    id: u64,
    #[serde(rename = "type")]
    pool_type: u8,
    balances: Vec<String>,
    spread_factor: String,
    token_out_denom: String,
    taker_fee: String,
}

async fn get_route(token_in: Token, token_out_denom: &str) -> Result<Vec<SwapAmountInRoute>> {
    let url = format!(
        "https://sqsprod.osmosis.zone/router/quote?tokenIn={}&tokenOutDenom={}",
        utf8_percent_encode(token_in.to_string().as_str(), NON_ALPHANUMERIC),
        token_out_denom
    );

    let res = reqwest::get(&url).await?;
    let txt = res.text().await?;
    let response: RouterResponse = serde_json::from_str(&txt).map_err(|e| {
        format!(
            "Failed to parse response from sqs: {}. Response: {}",
            e, txt
        )
    })?;

    // get route with the best out amount
    let route = response
        .route
        .iter()
        .max_by(|a, b| {
            a.out_amount
                .parse::<u128>()
                .unwrap()
                .cmp(&b.out_amount.parse::<u128>().unwrap())
        })
        .expect("No route found");

    let best_route = route
        .pools
        .iter()
        .map(|pool| SwapAmountInRoute {
            pool_id: pool.id,
            token_out_denom: pool.token_out_denom.clone(),
        })
        .collect::<Vec<_>>();

    Ok(best_route)
}
