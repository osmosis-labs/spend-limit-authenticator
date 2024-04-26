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
            .checked_sub(Uint64::from(self.last_updated_time.nanos()))
            .map_err(|_| {
                PriceError::current_block_time_behind_last_update(
                    block_time.nanos(),
                    self.last_updated_time.nanos(),
                )
            })?;

        Ok(duration_since_last_update >= staleness_threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::always_expire(0, 0, 0, Ok(true))]
    #[case::always_expire(1_708_416_816_000_000_000, last_updated_time, 0, Ok(true))]
    #[case::always_expire(1_708_416_816_000_000_000, last_updated_time + 1, 0, Ok(true))]
    #[case::should_expire(1_708_416_816_000_000_000, last_updated_time + 1, 1, Ok(true))]
    #[case::should_expire(1_708_416_816_000_000_000, last_updated_time + 3600_000000000, 3600_000000000, Ok(true))]
    #[case::should_expire(1_708_416_816_000_000_000, last_updated_time + 3600_000000001, 3600_000000000, Ok(true))]
    #[case::should_not_expire(1_708_416_816_000_000_000, last_updated_time, 1, Ok(false))]
    #[case::should_not_expire(1_708_416_816_000_000_000, last_updated_time + 3599_999999999, 3600_000000000, Ok(false))]
    #[case::invalid_block_time(1_708_416_816_000_000_000, last_updated_time - 1, 0, Err(
        PriceError::CurrentBlockTimeBehindLastUpdate {
            current_block_time: Uint64::from(block_time),
            last_updated_time: Uint64::from(last_updated_time)
        }
    ))]
    fn test_has_expired(
        #[case] last_updated_time: u64,
        #[case] block_time: u64,
        #[case] staleness_threshold: u64,
        #[case] expected: Result<bool, PriceError>,
    ) {
        let price_info = PriceInfo {
            price: Decimal::one(),
            last_updated_time: Timestamp::from_nanos(last_updated_time),
            swap_routes: vec![],
        };

        let block_time = Timestamp::from_nanos(block_time);
        let staleness_threshold = Uint64::from(staleness_threshold);

        assert_eq!(
            price_info.has_expired(block_time, staleness_threshold),
            expected
        );
    }
}
