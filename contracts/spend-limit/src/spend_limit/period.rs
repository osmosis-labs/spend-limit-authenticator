use cosmwasm_schema::cw_serde;

#[cw_serde]
pub enum Period {
    Day,
    Week,
    Month,
    Year,
}
