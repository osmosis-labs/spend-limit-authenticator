use cosmwasm_std::{Addr, OverflowError, Uint128};
use thiserror::Error;

use crate::period::PeriodError;

#[derive(Error, Debug, PartialEq)]
pub enum SpendLimitError {
    #[error("{0}")]
    Std(#[from] cosmwasm_std::StdError),

    #[error("Period error: {0}")]
    PeriodError(#[from] PeriodError),

    #[error("Spend limit not found for account {address} and authenticator {authenticator_id}")]
    SpendLimitNotFound {
        address: Addr,
        authenticator_id: String,
    },

    #[error("Accumulating spent value error: {0}")]
    AccumulatingSpentValueError(#[from] OverflowError),

    #[error("Overspend: {spent} has been spent but limit is {limit}")]
    Overspend { limit: Uint128, spent: Uint128 },
}

impl SpendLimitError {
    pub fn overspend(limit: u128, spent: u128) -> Self {
        Self::Overspend {
            limit: Uint128::from(limit),
            spent: Uint128::from(spent),
        }
    }
}

pub type SpendLimitResult<T> = Result<T, SpendLimitError>;
