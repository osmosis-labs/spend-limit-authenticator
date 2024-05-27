use cw_storage_plus::Item;

use crate::{
    admin::Admin,
    // passkey::,
};

/// Admin address, Optional.
pub const ADMIN: Item<Admin> = Item::new("admin");
