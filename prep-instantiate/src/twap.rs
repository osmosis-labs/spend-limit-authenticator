use cosmwasm_std::Decimal;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Iso8601;

use crate::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct ArithmeticTwapToNowResponse {
    pub arithmetic_twap: Decimal,
}

pub async fn arithmetic_twap_to_now(
    pool_id: u64,
    base_denom: &str,
    quote_denom: &str,
    start_time: time::OffsetDateTime,
) -> Result<Decimal> {
    let start_time = start_time.format(&Iso8601::DEFAULT)?;
    let url = format!(
        "https://lcd.osmosis.zone/osmosis/twap/v1beta1/ArithmeticTwapToNow?pool_id={}&base_asset={}&quote_asset={}&start_time={}",
        pool_id,
        base_denom,
        quote_denom,
        start_time
    );

    let response = reqwest::get(&url).await?;
    let text = response.text().await?;

    serde_json::from_str::<ArithmeticTwapToNowResponse>(&text)
        .map(|res| res.arithmetic_twap)
        .map_err(|e| {
            format!(
                "Failed to parse response from osmosis: {}. Response: {}",
                e, text
            )
            .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "slow & flaky test, use in dev only"]
    async fn test_arithmetic_twap_to_now() {
        let pool_id = 1223;
        let base_denom = "ibc/D189335C6E4A68B513C10AB227BF1C1D38C746766278BA3EEB4FB14124F1D858";
        let quote_denom = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
        let start_time = time::OffsetDateTime::now_utc()
            .checked_sub(time::Duration::hours(1))
            .unwrap();
        let res = arithmetic_twap_to_now(pool_id, base_denom, quote_denom, start_time).await;

        assert!(res.is_ok());
    }
}
