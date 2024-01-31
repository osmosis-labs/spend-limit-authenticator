use cosmwasm_std::{Timestamp, Uint128};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SpendLimitError {
    #[error("Overspent: remaining {remaining}, requested {requested}, limit reset timestamp {limit_reset_timestamp}")]
    OverSpent {
        remaining: Uint128,
        requested: Uint128,
        limit_reset_timestamp: Timestamp,
    },
}
