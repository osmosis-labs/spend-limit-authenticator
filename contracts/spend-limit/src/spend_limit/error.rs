use super::period::PeriodError;
use cosmwasm_std::{OverflowError, Uint128};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SpendLimitError {
    #[error("{0}")]
    Std(#[from] cosmwasm_std::StdError),

    #[error("Period error: {0}")]
    PeriodError(#[from] PeriodError),

    #[error("Accumulating spent value error: {0}")]
    AccumulatingSpentValueError(#[from] OverflowError),

    #[error("Overspent: remaining qouta {remaining}, requested {requested}")]
    Overspent {
        remaining: Uint128,
        requested: Uint128,
        // TODO: add `reset_at: OffsetDateTime`
    },
}

impl SpendLimitError {
    pub fn overspent(remaining: u128, requested: u128) -> Self {
        Self::Overspent {
            remaining: Uint128::from(remaining),
            requested: Uint128::from(requested),
        }
    }
}

pub type SpendLimitResult<T> = Result<T, SpendLimitError>;
