use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Uint64};

use super::period::Period;

#[cw_serde]
pub struct SpendLimitParams {
    /// Authenticator id of this spend limit
    /// TODO: After authenticator id is passed in as a request, we can remove this field
    pub authenticator_id: Uint64,

    /// Limit per period, used to enforce spend limit with this given amount and denom.
    /// The denom is used as quote currency for the spend limit.
    pub limit: Coin,

    /// Period to reset spend limit quota
    pub reset_period: Period,
}
