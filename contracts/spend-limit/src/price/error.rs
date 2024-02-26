use cosmwasm_std::{OverflowError, Timestamp, Uint64};
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum PriceError {
    #[error("{0}")]
    StdError(#[from] cosmwasm_std::StdError),

    #[error("Swap routes must end with quote denom: {quote_denom}, but got swap_routes: {swap_routes:?}")]
    SwapRoutesMustEndWithQuoteDenom {
        quote_denom: String,
        swap_routes: Vec<SwapAmountInRoute>,
    },

    #[error("Price calculation error: {0}")]
    PriceCalculationError(#[from] OverflowError),

    #[error("Invalid block time: current block time `{current_block_time}`, is behind last updated time `{last_updated_time}`")]
    CurrentBlockTimeBehindLastUpdate {
        current_block_time: Uint64,
        last_updated_time: Uint64,
    },

    #[error(
        "Twap query error: can't get twap price for {base_denom}:{quote_denom} from pool {pool_id}, starting from {start_time} to now"
    )]
    TwapQueryError {
        pool_id: u64,
        base_denom: String,
        quote_denom: String,
        start_time: Timestamp,
    },
}

impl PriceError {
    pub fn current_block_time_behind_last_update(
        current_block_time: u64,
        last_updated_time: u64,
    ) -> Self {
        PriceError::CurrentBlockTimeBehindLastUpdate {
            current_block_time: current_block_time.into(),
            last_updated_time: last_updated_time.into(),
        }
    }

    pub fn twap_query_error(
        pool_id: u64,
        base_denom: &str,
        quote_denom: &str,
        start_time: Timestamp,
    ) -> Self {
        PriceError::TwapQueryError {
            pool_id,
            base_denom: base_denom.to_string(),
            quote_denom: quote_denom.to_string(),
            start_time,
        }
    }
}
