use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Timestamp, Uint128};

use super::period::Period;

#[cw_serde]
pub struct SpendLimitParams {
    /// Limit per period, used to enforce spend limit with this given amount in quote denom
    pub limit: Uint128,

    /// Period to reset spend limit quota
    pub reset_period: Period,

    /// Time limit for the spend limit
    pub time_limit: Option<TimeLimit>,
}

#[cw_serde]
pub struct TimeLimit {
    /// Start time of the time limit, if not set, it means the time limit starts immediately
    pub start: Option<Timestamp>,

    /// End time of the time limit
    pub end: Timestamp,
}
