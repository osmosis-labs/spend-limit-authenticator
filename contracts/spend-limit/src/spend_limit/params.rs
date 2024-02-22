use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use super::period::Period;

#[cw_serde]
pub struct SpendLimitParams {
    /// Limit per period, used to enforce spend limit with this given amount in quote denom
    pub limit: Uint128,

    /// Period to reset spend limit quota
    pub reset_period: Period,
}
