#![no_std]

mod contract;

pub use contract::{
    CoreContract, CoreContractClient, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionStatus,
};

#[cfg(test)]
mod test;
