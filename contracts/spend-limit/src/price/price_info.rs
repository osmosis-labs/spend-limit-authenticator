use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Timestamp, Uint64};
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

use super::PriceError;

#[cw_serde]
pub struct PriceInfo {
    /// Price of the asset
    pub price: Decimal,

    /// Timestamp when the price was last updated
    pub last_updated_time: Timestamp,

    /// Paths used to calculate the price
    pub swap_routes: Vec<SwapAmountInRoute>,
}

impl PriceInfo {
    pub fn has_expired(
        &self,
        block_time: Timestamp,
        staleness_threshold: Uint64,
    ) -> Result<bool, PriceError> {
        let duration_since_last_update = Uint64::from(block_time.nanos())
            .checked_sub(Uint64::from(self.last_updated_time.nanos()))?;

        Ok(duration_since_last_update > staleness_threshold)
    }
}
