use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Map;

pub type SpendLimitStorage<'a> = Map<'a, SpendLimitKey<'a>, AmountSpent>;

/// SpendLimitKey is a key for the spend limit state
/// It is a tuple of (account, subkey)
pub struct SpendLimitKey<'a>(&'a Addr, &'a str);

impl<'a> SpendLimitKey<'a> {
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

pub type AmountSpent = Uint128;
