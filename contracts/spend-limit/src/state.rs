use cw_storage_plus::{Item, Map};

use crate::{
    admin::Admin,
    fee::UntrackedSpentFeeStore,
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

/// Fee that has been spent but not yet tracked as spending.
/// This is required for failed transactions because if the transaction fails after ante handlers,
/// the fee is still deducted from the account but the spending is not tracked.
/// In that case, we need to accumulate the fee in this storage and assert the limit later
/// to prevent fee draining.
pub const UNTRACKED_SPENT_FEES: UntrackedSpentFeeStore<'_> = Map::new("untracked_spent_fees");

/// Configuration for the price resolution.
pub const PRICE_RESOLUTION_CONFIG: Item<PriceResolutionConfig> =
    Item::new("price_resolution_config");

/// Store for the price info of the tracked denoms.
pub const PRICE_INFOS: PriceInfoStore<'_> = Map::new("price_infos");

/// Admin address, Optional.
pub const ADMIN: Item<Admin> = Item::new("admin");
