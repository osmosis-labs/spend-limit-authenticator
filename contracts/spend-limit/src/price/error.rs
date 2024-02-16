use cosmwasm_std::OverflowError;
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;
use thiserror::Error;
use time::error::ComponentRange;

#[derive(Error, Debug, PartialEq)]
pub enum PriceError {
    #[error("{0}")]
    StdError(#[from] cosmwasm_std::StdError),

    #[error("Swap routes must end with quote denom: {qoute_denom}, but got swap_routes: {swap_routes:?}")]
    SwapRoutesMustEndWithQuoteDenom {
        qoute_denom: String,
        swap_routes: Vec<SwapAmountInRoute>,
    },

    #[error("Price calculation error: {0}")]
    PriceCalculationError(#[from] OverflowError),

    #[error("Converting cosmwasm_std timestamp to offset datetime failed: {0}")]
    TimestampConversionError(#[from] ComponentRange),
}
