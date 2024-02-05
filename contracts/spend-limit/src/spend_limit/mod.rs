mod error;
mod period;
mod spending;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;
use period::Period;
use spending::Spending;

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

/// TransientBalanceTracker is a map of spending keys to the coins spent.
pub type TransientBalanceTracker<'a> = Map<'a, SpendingKey<'a>, Vec<Coin>>;

/// SpendingKey is a key for the spending storage.
/// It is a tuple of (account, subkey) which
/// allows multiple spend limits per account.
pub struct SpendingKey<'a>(&'a Addr, &'a str);

impl<'a> SpendingKey<'a> {
    pub fn new(addr: &'a Addr, subkey: &'a str) -> Self {
        Self(addr, subkey)
    }

    pub fn account(&self) -> &Addr {
        self.0
    }

    pub fn subkey(&self) -> &str {
        self.1
    }
}

#[cw_serde]
pub struct SpendLimitParams {
    /// Subkey for the account, to allow multiple spend limits per account
    subkey: String,

    /// Limit per period, used to enforce spend limit with this given amount and denom.
    /// The denom is used as quote currency for the spend limit.
    limit: Coin,

    /// Period to reset spend limit quota
    reset_period: Period,
}
