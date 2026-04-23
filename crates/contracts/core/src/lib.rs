#![no_std]

mod contract;

pub use contract::{
    CoreContract, CoreContractClient, DisputeResolvedEvent, FeeDeductedEvent, FundsLockedEvent,
    InitializedEvent, RefundEvent, Session, SessionApprovedEvent, SessionCompletedEvent,
    SessionStatus,
};

#[cfg(test)]
mod test;
