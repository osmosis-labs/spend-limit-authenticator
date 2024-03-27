use crate::Result;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use spend_limit::msg::SwapAmountInRoute;
use std::fmt::Display;

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
pub struct Token {
    pub denom: String,
    pub amount: String,
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

#[repr(u8)]
#[derive(Debug, Serialize, Deserialize)]
enum PoolType {
    // Balancer is the standard xy=k curve. Its pool model is defined in x/gamm.
    Balancer = 0,
    // Stableswap is the Solidly cfmm stable swap curve. Its pool model is defined
    // in x/gamm.
    Stableswap = 1,
    // Concentrated is the pool model specific to concentrated liquidity. It is
    // defined in x/concentrated-liquidity.
    Concentrated = 2,
    // CosmWasm is the pool model specific to CosmWasm. It is defined in
    // x/cosmwasmpool.
    CosmWasm = 3,
}

impl From<u8> for PoolType {
    fn from(v: u8) -> Self {
        match v {
            0 => PoolType::Balancer,
            1 => PoolType::Stableswap,
            2 => PoolType::Concentrated,
            3 => PoolType::CosmWasm,
            _ => panic!("Unknown pool type: {}", v),
        }
    }
}

pub async fn get_route(token_in: Token, token_out_denom: &str) -> Result<Vec<SwapAmountInRoute>> {
    let url = format!(
        "https://sqsprod.osmosis.zone/router/quote?tokenIn={}&tokenOutDenom={}",
        utf8_percent_encode(token_in.to_string().as_str(), NON_ALPHANUMERIC),
        token_out_denom
    );

    let res = reqwest::get(&url).await?;
    let txt = res.text().await?;
    let response: RouterResponse = serde_json::from_str(&txt).map_err(|e| {
        format!(
            "Failed to parse response from sqs: {}. Denom: {}, Response: {}",
            e, token_in.denom, txt
        )
    })?;

    // get route with the best out amount
    let route = response
        .route
        .iter()
        .filter(|r| {
            let res = !r.has_cw_pool;

            if !res {
                eprintln!("Route contains cw-pool which can't get twap price:");
                eprintln!("has_cw_pool: {}", r.has_cw_pool);
                eprintln!("  - {}", token_in.denom);
                for pool in &r.pools {
                    eprintln!(
                        "  - [{} ~ {:?}] -> {}",
                        pool.id,
                        PoolType::from(pool.pool_type),
                        pool.token_out_denom
                    );
                }
            }

            res
        }) // filter out the pool with the CW pool since it's not twap-able
        .max_by(|a, b| {
            a.out_amount
                .parse::<u128>()
                .unwrap()
                .cmp(&b.out_amount.parse::<u128>().unwrap())
        });

    match route {
        None => Ok(vec![]),
        Some(route) => {
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
    }
}
