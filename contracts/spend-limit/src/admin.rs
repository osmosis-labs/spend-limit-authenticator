use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr};

use crate::ContractError;

#[cw_serde]
pub enum Admin {
    Settled(Addr),
    Transferring { current: Addr, candidate: Addr },
    None,
}

impl Admin {
    pub fn new(address: Addr) -> Self {
        Admin::Settled(address)
    }

    pub fn admin(&self) -> Option<&Addr> {
        match self {
            Admin::Settled(current) => Some(current),
            Admin::Transferring { current, .. } => Some(current),
            Admin::None => None,
        }
    }

    pub fn admin_once(self) -> Option<Addr> {
        match self {
            Admin::Settled(current) => Some(current),
            Admin::Transferring { current, .. } => Some(current),
            Admin::None => None,
        }
    }

    pub fn candidate(&self) -> Option<&Addr> {
        match self {
            Admin::Transferring { candidate, .. } => Some(candidate),
            _ => None,
        }
    }

    pub fn candidate_once(self) -> Option<Addr> {
        match self {
            Admin::Transferring { candidate, .. } => Some(candidate),
            _ => None,
        }
    }

    pub fn authorize_admin(&self, addr: &Addr) -> Result<(), ContractError> {
        ensure!(Some(addr) == self.admin(), ContractError::Unauthorized {});
        Ok(())
    }

    pub fn authorize_candidate(&self, addr: &Addr) -> Result<(), ContractError> {
        ensure!(
            Some(addr) == self.candidate(),
            ContractError::Unauthorized {}
        );
        Ok(())
    }

    pub fn authorized_revoke_admin(self, sender: &Addr) -> Result<Self, ContractError> {
        self.authorize_admin(sender)?;

        Ok(Admin::None)
    }

    pub fn authorized_transfer_admin(
        self,
        sender: &Addr,
        candidate: Addr,
    ) -> Result<Self, ContractError> {
        self.authorize_admin(sender)?;

        match self {
            Admin::Settled(current) => Ok(Admin::Transferring { current, candidate }),
            Admin::Transferring {
                current,
                candidate: _old_candidate,
            } => Ok(Admin::Transferring { current, candidate }),
            Admin::None => Err(ContractError::Unauthorized {}),
        }
    }

    pub fn authorized_claim_admin_transfer(self, sender: &Addr) -> Result<Self, ContractError> {
        self.authorize_candidate(sender)?;

        match self {
            Admin::Transferring { candidate, .. } => Ok(Admin::Settled(candidate)),
            _ => Err(ContractError::Unauthorized {}),
        }
    }

    pub fn authorized_reject_admin_transfer(self, sender: &Addr) -> Result<Self, ContractError> {
        self.authorize_candidate(sender)?;

        match self {
            Admin::Transferring { current, .. } => Ok(Admin::Settled(current)),
            _ => Err(ContractError::Unauthorized {}),
        }
    }

    pub fn authorized_cancel_admin_transfer(self, sender: &Addr) -> Result<Self, ContractError> {
        self.authorize_admin(sender)?;

        match self {
            Admin::Transferring { current, .. } => Ok(Admin::Settled(current)),
            _ => Err(ContractError::Unauthorized {}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Addr;

    #[test]
    fn admin_retrieval() {
        let admin = Addr::unchecked("admin");
        let candidate = Addr::unchecked("candidate");
        let admin_settled = Admin::Settled(admin.clone());
        assert_eq!(admin_settled.admin(), Some(&admin));

        let admin_transferring = Admin::Transferring {
            current: admin.clone(),
            candidate: candidate.clone(),
        };
        assert_eq!(admin_transferring.admin(), Some(&admin));

        let admin_none = Admin::None;
        assert_eq!(admin_none.admin(), None);
    }

    #[test]
    fn candidate_retrieval() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current: current.clone(),
            candidate: candidate.clone(),
        };
        assert_eq!(admin.candidate(), Some(&candidate));

        let admin_settled = Admin::Settled(current.clone());
        assert_eq!(admin_settled.candidate(), None);

        let admin_none = Admin::None;
        assert_eq!(admin_none.candidate(), None);
    }

    #[test]
    fn ensure_admin_success() {
        let addr = Addr::unchecked("admin");
        let admin = Admin::Settled(addr.clone());
        assert!(admin.authorize_admin(&addr).is_ok());
    }

    #[test]
    fn ensure_admin_failure() {
        let addr = Addr::unchecked("admin");
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin.authorize_admin(&other_addr).is_err());

        let admin = Admin::Transferring {
            current: current.clone(),
            candidate,
        };
        assert!(admin.authorize_admin(&other_addr).is_err());

        let admin = Admin::None;
        assert!(admin.authorize_admin(&other_addr).is_err());
    }

    #[test]
    fn ensure_candidate_success() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current,
            candidate: candidate.clone(),
        };
        assert!(admin.authorize_candidate(&candidate).is_ok());
    }

    #[test]
    fn ensure_candidate_failure() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin_transferring = Admin::Transferring {
            current: current.clone(),
            candidate: candidate.clone(),
        };
        assert!(admin_transferring.authorize_candidate(&other_addr).is_err());

        let admin_settled = Admin::Settled(current);
        assert!(admin_settled.authorize_candidate(&other_addr).is_err());

        let admin_none = Admin::None;
        assert!(admin_none.authorize_candidate(&other_addr).is_err());
    }

    #[test]
    fn revoke_admin_success() {
        let addr = Addr::unchecked("admin");
        let admin = Admin::Settled(addr.clone());
        assert!(matches!(
            admin.authorized_revoke_admin(&addr),
            Ok(Admin::None)
        ));
    }

    #[test]
    fn revoke_admin_failure() {
        let addr = Addr::unchecked("admin");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin.authorized_revoke_admin(&other_addr).is_err());
    }

    #[test]
    fn transfer_admin_success() {
        let addr = Addr::unchecked("admin");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Settled(addr.clone());
        assert!(matches!(
            admin.authorized_transfer_admin(&addr, candidate.clone()),
            Ok(Admin::Transferring {
                current: _,
                candidate: _
            })
        ));
    }

    #[test]
    fn transfer_admin_success_on_transferring_state() {
        let addr = Addr::unchecked("admin");
        let prev_candidate = Addr::unchecked("prev_candidate");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current: addr.clone(),
            candidate: prev_candidate,
        };
        assert_eq!(
            admin.authorized_transfer_admin(&addr, candidate.clone()),
            Ok(Admin::Transferring {
                current: addr,
                candidate,
            })
        );
    }

    #[test]
    fn transfer_admin_failure() {
        let addr = Addr::unchecked("admin");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin
            .authorized_transfer_admin(&other_addr, candidate)
            .is_err());
    }

    #[test]
    fn claim_admin_success() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current,
            candidate: candidate.clone(),
        };
        assert!(matches!(
            admin.authorized_claim_admin_transfer(&candidate),
            Ok(Admin::Settled(_))
        ));
    }

    #[test]
    fn claim_admin_failure() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Transferring { current, candidate };
        assert!(admin.authorized_claim_admin_transfer(&other_addr).is_err());
    }

    #[test]
    fn reject_admin_success() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current,
            candidate: candidate.clone(),
        };
        assert!(matches!(
            admin.authorized_reject_admin_transfer(&candidate),
            Ok(Admin::Settled(_))
        ));
    }

    #[test]
    fn reject_admin_failure() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Transferring { current, candidate };
        assert!(admin.authorized_reject_admin_transfer(&other_addr).is_err());
    }

    #[test]
    fn cancel_transfer_success() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current: current.clone(),
            candidate: candidate.clone(),
        };
        assert_eq!(
            admin.authorized_cancel_admin_transfer(&current),
            Ok(Admin::Settled(current))
        );
    }

    #[test]
    fn cancel_transfer_failure() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Transferring { current, candidate };
        assert!(admin.authorized_cancel_admin_transfer(&other_addr).is_err());
    }
}
