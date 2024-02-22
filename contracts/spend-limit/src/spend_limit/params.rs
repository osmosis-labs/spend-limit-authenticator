use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

use super::period::Period;

#[cw_serde]
pub struct SpendLimitParams {
    /// Limit per period, used to enforce spend limit with this given amount and denom.
    /// The denom is used as quote currency for the spend limit.
    pub limit: Coin,

    /// Period to reset spend limit quota
    pub reset_period: Period,
}
