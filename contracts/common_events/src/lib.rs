#![cfg_attr(not(feature = "std"), no_std)]

use ink::primitives::AccountId;

/// Common Event Schemas for SkillSync Contracts
/// These structs represent the standardized data layout for backend indexing.
pub mod schemas {
    use super::*;

    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct BookingFunded {
        pub booking_id: u64,
        pub mentee: AccountId,
        pub mentor: AccountId,
        pub token: AccountId,
        pub amount: u128,
    }

    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct BookingReleased {
        pub booking_id: u64,
        pub mentor: AccountId,
        pub token: AccountId,
        pub amount: u128,
    }

    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct BookingRefunded {
        pub booking_id: u64,
        pub mentee: AccountId,
        pub token: AccountId,
        pub amount: u128,
    }

    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct DisputeOpened {
        pub booking_id: u64,
        pub initiator: AccountId,
        pub reason_hash: [u8; 32],
    }

    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct DisputeResolved {
        pub booking_id: u64,
        pub resolution: u8, // Enum mapped to u8
    }

    // Matches release_auth contract event
    #[derive(scale::Encode, scale::Decode, Debug, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub struct ReleaseAuthorized {
        pub booking_id: u64,
        pub mentor: AccountId,
        pub signer: [u8; 32],
    }
}
