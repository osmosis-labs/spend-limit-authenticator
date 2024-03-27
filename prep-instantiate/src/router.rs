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

pub async fn get_route(
    token_in: Token,
    token_out_denom: &str,
    latest_sycned_pool: Option<u64>,
    rejected_pool_ids: &[u64],
) -> Result<Vec<SwapAmountInRoute>> {
    // TODO: use `/router/route` instead of `/router/qoute`.
    // blocked by: https://linear.app/osmosis/issue/STABI-41/[bug]-403-on-routerroutes
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
        // filter out the pool with the CW pool since it's not twap-able
        .filter(|r| {
            let has_cw_pool = r
                .pools
                .iter()
                .any(|pool| pool.pool_type == PoolType::CosmWasm as u8);

            // check wheter in non-mainnet, there is any pool that is not yet synced
            let has_unsycned_pool = latest_sycned_pool
                .map(|latest| r.pools.iter().any(|pool| pool.id > latest))
                .unwrap_or(false);

            let has_rejected_pool = r
                .pools
                .iter()
                .any(|pool| rejected_pool_ids.contains(&pool.id));

            !has_cw_pool && !has_unsycned_pool && !has_rejected_pool
        })
        // mininum hops route leads to cheaper twap cost
        .min_by(|a, b| a.pools.len().cmp(&b.pools.len()));

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
