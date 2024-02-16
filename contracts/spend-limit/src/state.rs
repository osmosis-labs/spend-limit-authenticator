use cw_storage_plus::{Item, Map};

use crate::{
    price::{PriceInfoStore, PriceResolutionConfig},
    spend_limit::{PreExecBalance, SpendingStore},
};

pub const SPENDINGS: SpendingStore<'_> = Map::new("spendings");

/// [`PreExecBalance`] is a map of spending keys to the account balances.
/// It is used to track the balances of the accounts before the transaction is executed,
/// and compare it with the balances after the transaction is executed.
///
/// It's lifetime is only within one authenticator's lifecycle.
pub const PRE_EXEC_BALANCES: PreExecBalance<'_> = Map::new("pre_exec_balance");

/// Configuration for the price resolution.
pub const PRICE_RESOLUTION_CONFIG: Item<PriceResolutionConfig> =
    Item::new("price_resolution_config");

/// Store for the price info of the tracked denoms.
pub const PRICE_INFOS: PriceInfoStore<'_> = Map::new("price_infos");
