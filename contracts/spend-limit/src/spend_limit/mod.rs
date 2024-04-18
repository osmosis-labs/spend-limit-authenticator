mod error;
mod params;
mod spending;
use cosmwasm_std::Coin;

pub use error::SpendLimitError;
pub use params::{SpendLimitParams, TimeLimit};
pub use spending::{calculate_received_coins, calculate_spent_coins, Spending};

use cosmwasm_std::Addr;
use cw_storage_plus::Map;

pub type SpendingStore<'a> = Map<'a, SpendingKey<'a>, Spending>;

/// [`PreExecBalance`] is a map of spending keys to the coins spent.
pub type PreExecBalance<'a> = Map<'a, SpendingKey<'a>, Vec<Coin>>;

/// SpendingKey is a key for the spending storage.
/// It is a tuple of (account, authenticator_id) which
/// allows multiple spend limits per account.
pub type SpendingKey<'a> = (&'a Addr, &'a str);
