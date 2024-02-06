mod error;
mod params;
mod period;
mod spending;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

pub use error::SpendLimitError;
pub use params::SpendLimitParams;
pub use spending::{calculate_spent_coins, Spending};

#[cw_serde]
pub struct DeprecatedSpendLimit {
    pub id: String,
    pub denom: String,
    pub balance: Vec<Coin>,
    pub amount_left: u128,
    pub block_of_last_tx: u64,
    pub number_of_blocks_active: u64,
}

// -------------------------------------------

use cosmwasm_std::Addr;
use cw_storage_plus::Map;

pub type SpendingStorage<'a> = Map<'a, SpendingKey<'a>, Spending>;

/// [`TransientBalance`] is a map of spending keys to the coins spent.
pub type TransientBalance<'a> = Map<'a, SpendingKey<'a>, Vec<Coin>>;

/// SpendingKey is a key for the spending storage.
/// It is a tuple of (account, subkey) which
/// allows multiple spend limits per account.
pub type SpendingKey<'a> = (&'a Addr, &'a str);
