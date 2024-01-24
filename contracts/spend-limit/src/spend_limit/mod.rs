// TODO: Move core spend limit logic here

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Coin;

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
