use cosmwasm_std::{ensure, Addr};

use crate::ContractError;

pub enum Admin {
    Settled(Addr),
    Transferring { current: Addr, candidate: Addr },
    None,
}

impl Admin {
    pub fn admin(&self) -> Option<&Addr> {
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

    pub fn ensure_admin(&self, addr: &Addr) -> Result<(), ContractError> {
        ensure!(Some(addr) == self.admin(), ContractError::Unauthorized {});
        Ok(())
    }

    pub fn ensure_candidate(&self, addr: &Addr) -> Result<(), ContractError> {
        ensure!(
            Some(addr) == self.candidate(),
            ContractError::Unauthorized {}
        );
        Ok(())
    }

    pub fn revoke_admin(self, sender: &Addr) -> Result<Self, ContractError> {
        self.ensure_admin(sender)?;

        Ok(Admin::None)
    }

    pub fn transfer_admin(self, sender: &Addr, candidate: Addr) -> Result<Self, ContractError> {
        self.ensure_admin(sender)?;

        match self {
            Admin::Settled(current) => Ok(Admin::Transferring { current, candidate }),
            Admin::Transferring { current, candidate } => {
                Ok(Admin::Transferring { current, candidate })
            }
            Admin::None => Err(ContractError::Unauthorized {}),
        }
    }

    pub fn claim_admin(self, sender: &Addr) -> Result<Self, ContractError> {
        self.ensure_candidate(sender)?;

        match self {
            Admin::Transferring { candidate, .. } => Ok(Admin::Settled(candidate)),
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
        assert!(admin.ensure_admin(&addr).is_ok());
    }

    #[test]
    fn ensure_admin_failure() {
        let addr = Addr::unchecked("admin");
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin.ensure_admin(&other_addr).is_err());

        let admin = Admin::Transferring {
            current: current.clone(),
            candidate,
        };
        assert!(admin.ensure_admin(&other_addr).is_err());

        let admin = Admin::None;
        assert!(admin.ensure_admin(&other_addr).is_err());
    }

    #[test]
    fn ensure_candidate_success() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Transferring {
            current,
            candidate: candidate.clone(),
        };
        assert!(admin.ensure_candidate(&candidate).is_ok());
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
        assert!(admin_transferring.ensure_candidate(&other_addr).is_err());

        let admin_settled = Admin::Settled(current);
        assert!(admin_settled.ensure_candidate(&other_addr).is_err());

        let admin_none = Admin::None;
        assert!(admin_none.ensure_candidate(&other_addr).is_err());
    }

    #[test]
    fn revoke_admin_success() {
        let addr = Addr::unchecked("admin");
        let admin = Admin::Settled(addr.clone());
        assert!(matches!(admin.revoke_admin(&addr), Ok(Admin::None)));
    }

    #[test]
    fn revoke_admin_failure() {
        let addr = Addr::unchecked("admin");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin.revoke_admin(&other_addr).is_err());
    }

    #[test]
    fn transfer_admin_success() {
        let addr = Addr::unchecked("admin");
        let candidate = Addr::unchecked("candidate");
        let admin = Admin::Settled(addr.clone());
        assert!(matches!(
            admin.transfer_admin(&addr, candidate.clone()),
            Ok(Admin::Transferring {
                current: _,
                candidate: _
            })
        ));
    }

    #[test]
    fn transfer_admin_failure() {
        let addr = Addr::unchecked("admin");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Settled(addr);
        assert!(admin.transfer_admin(&other_addr, candidate).is_err());
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
            admin.claim_admin(&candidate),
            Ok(Admin::Settled(_))
        ));
    }

    #[test]
    fn claim_admin_failure() {
        let current = Addr::unchecked("current");
        let candidate = Addr::unchecked("candidate");
        let other_addr = Addr::unchecked("other");
        let admin = Admin::Transferring { current, candidate };
        assert!(admin.claim_admin(&other_addr).is_err());
    }
}
