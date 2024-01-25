// TODO: Move core spend limit logic here

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin};

// The idea is:
// - track spend limit overtime
// - value is denominated in a specific coin
// - another price oracle conrtact is used to keep track of a cached the price of the coin
#[cw_serde]
pub struct SpendLimit {
    pub id: String,
    pub denom: String,
    pub balance: Vec<Coin>,
    pub amount_left: u128,
    pub block_of_last_tx: u64,
    pub number_of_blocks_active: u64,
}

#[cw_serde]
pub enum PriceStrategy {
    /// Using just the amount of the given denom to determine quota.
    AbsoluteValue,

    /// Using a price oracle contract to determine the price of the coin.
    /// Spending can be done in any coin, as long as price oracle contract provides the price.
    ///
    /// The contract must implement the following query:
    /// - request:  `{ "get_price" { "denom": "<denom>" } }`
    /// - response: `{ "price": "<price>" }`
    PriceOracle { contract_address: Addr },
}
