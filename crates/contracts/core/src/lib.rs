#![no_std]

mod contract;

pub use contract::{
<<<<<<< main
    ContractError, CoreContract, CoreContractClient, LockedSession, Session,
    SessionApprovedEvent, SessionCompletedEvent, SessionStatus,
=======
    CoreContract, CoreContractClient, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionStatus,
>>>>>>> main
};

#[cfg(test)]
mod test;
