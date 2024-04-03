use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

use crate::Result;
use cosmwasm_std::Coin;
use serde::{Deserialize, Deserializer, Serialize};

#[repr(u8)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PoolType {
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

impl Display for PoolType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            PoolType::Balancer => write!(f, "BL"),
            PoolType::Stableswap => write!(f, "SS"),
            PoolType::Concentrated => write!(f, "CL"),
            PoolType::CosmWasm => write!(f, "CW"),
        }
    }
}

pub fn deserialize_pool_type<'de, D>(deserializer: D) -> std::result::Result<PoolType, D::Error>
where
    D: Deserializer<'de>,
{
    let pool_type_u8 = u8::deserialize(deserializer)?;
    Ok(match pool_type_u8 {
        0 => PoolType::Balancer,
        1 => PoolType::Stableswap,
        2 => PoolType::Concentrated,
        3 => PoolType::CosmWasm,
        _ => return Err(serde::de::Error::custom("Invalid pool type")),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainModel {
    #[serde(alias = "pool_id")]
    pub id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SQSPoolInfo {
    pub chain_model: ChainModel,

    pub balances: Vec<Coin>,

    #[serde(rename = "type", deserialize_with = "deserialize_pool_type")]
    pub pool_type: PoolType,
}

impl SQSPoolInfo {
    /// CosmWasm pools are not currently supported for TWAP calculations.
    pub fn is_twap_supported(&self) -> bool {
        matches!(
            self.pool_type,
            PoolType::Balancer | PoolType::Stableswap | PoolType::Concentrated
        )
    }
}

pub async fn get_pools() -> Result<HashMap<u64, SQSPoolInfo>> {
    let res = reqwest::get("https://sqsprod.osmosis.zone/pools").await?;
    let txt = res.text().await?;

    Ok(serde_json::from_str::<Vec<SQSPoolInfo>>(&txt)
        .map_err(|e| {
            format!(
                "Failed to parse pool infos from response: {}. Response: {}",
                e, txt
            )
        })?
        .into_iter()
        .map(|pool_info| (pool_info.chain_model.id, pool_info))
        .collect::<HashMap<_, _>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "slow & flaky test, use in dev only"]
    async fn test_pools() {
        let res = get_pools().await;

        assert!(res.is_ok());
    }
}
