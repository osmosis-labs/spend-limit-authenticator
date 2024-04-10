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

    #[error("Overspend: remaining quota {remaining}, requested {requested}")]
    Overspend {
        remaining: Uint128,
        requested: Uint128,
    },
}

impl SpendLimitError {
    pub fn overspend(remaining: u128, requested: u128) -> Self {
        Self::Overspend {
            remaining: Uint128::from(remaining),
            requested: Uint128::from(requested),
        }
    }
}

pub type SpendLimitResult<T> = Result<T, SpendLimitError>;
