use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

use super::period::Period;

#[cw_serde]
pub struct SpendLimitParams {
    /// Subkey for the account, to allow multiple spend limits per account
    /// TODO: After authenticator id is passed in as a request, we can remove this field
    pub subkey: String,

    /// Limit per period, used to enforce spend limit with this given amount and denom.
    /// The denom is used as quote currency for the spend limit.
    pub limit: Coin,

    /// Period to reset spend limit quota
    pub reset_period: Period,
}
