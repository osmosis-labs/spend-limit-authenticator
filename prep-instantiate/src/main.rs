use std::fmt::Display;

use error_chain::error_chain;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

error_chain! {
    foreign_links {
        Io(std::io::Error);
        HttpRequest(reqwest::Error);
        Json(serde_json::Error);
    }
}

use serde::{Deserialize, Serialize};
use spend_limit::{
    msg::{InstantiateMsg, SwapAmountInRoute, TrackedDenom},
    price::PriceResolutionConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    let price_resolution_config = PriceResolutionConfig {
        quote_denom: "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4"
            .to_string(),
        staleness_threshold: 3_600_000_000_000u64.into(),
        twap_duration: 3_600_000_000_000u64.into(),
    };

    let mut tracked_denoms = vec![];

    for denom in vec!["uosmo"] {
        let route = TrackedDenom {
            denom: denom.to_string(),
            swap_routes: get_route(
                Token::new(5000000000, denom),
                &price_resolution_config.quote_denom,
            )
            .await?,
        };
        tracked_denoms.push(route);
    }

    let msg = InstantiateMsg {
        price_resolution_config,
        tracked_denoms,
    };

    // write instantiate msg to stdout
    let msg_str = serde_json::to_string(&msg)?;
    println!("{}", msg_str);

    Ok(())
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
    let response: RouterResponse = serde_json::from_str(&txt)?;

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
