use super::period::PeriodError;
use cosmwasm_std::{OverflowError, Uint128};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SpendLimitError {
    #[error("Period error: {0}")]
    PeriodError(#[from] PeriodError),

    #[error("Accumulating spent value error: {0}")]
    AccumulatingSpentValueError(#[from] OverflowError),

    #[error("Overspent: remaining qouta {remaining}, requested {requested}")]
    OverSpent {
        remaining: Uint128,
        requested: Uint128,
        // TODO: add `reset_at: OffsetDateTime`
    },
}

pub type SpendLimitResult<T> = Result<T, SpendLimitError>;
