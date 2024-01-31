use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

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
