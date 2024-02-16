use cosmwasm_std::OverflowError;
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
}
