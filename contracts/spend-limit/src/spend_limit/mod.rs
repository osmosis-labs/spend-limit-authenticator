mod error;
mod period;
mod price;
mod storage;

pub use storage::SpendLimitStorage;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

use period::Period;
use price::PriceStrategy;

#[cw_serde]
pub struct DeprecatedSpendLimit {
    pub id: String,
    pub denom: String,
    pub balance: Vec<Coin>,
    pub amount_left: u128,
    pub block_of_last_tx: u64,
    pub number_of_blocks_active: u64,
}

#[cw_serde]
pub struct SpendLimitParams {
    /// Subkey for the account, to allow multiple spend limits per account
    subkey: String,

    /// limit per period
    /// if the price strategy is absolute value, this requires no conversion
    /// if the price strategy is price oracle, this requires conversion and this coin denom is used as the quote denom
    limit: Coin,

    /// Period to reset spend limit quota
    reset_period: Period,

    /// Price strategy to determine limit quota
    price_strategy: PriceStrategy,
}
