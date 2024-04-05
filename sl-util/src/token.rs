use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenInfo {
    pub price: f64,
    pub denom: String,
    pub symbol: String,
    pub liquidity: f64,
    pub volume_24h: f64,
    pub volume_24h_change: f64,
    pub name: String,
    pub price_24h_change: f64,
    pub price_7d_change: f64,
    pub exponent: u32,
    pub display: String,
}

pub async fn get_tokens() -> Result<Vec<TokenInfo>> {
    let url = "https://api-osmosis.imperator.co/tokens/v2/all";
    let res = reqwest::get(url).await?;
    let txt = res.text().await?;
    let response: Vec<TokenInfo> = serde_json::from_str(&txt).map_err(|e| {
        format!(
            "Failed to parse token infos from response: {}. Response: {}",
            e, txt
        )
    })?;

    Ok(response)
}
