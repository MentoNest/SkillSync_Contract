use soroban_sdk::{contracttype, Address, Bytes};

/// Emitted when an admin resolves a dispute (issue #150).
///
/// Shows the final distribution of funds between buyer and seller,
/// the platform fee taken, and the timestamp of resolution.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeResolved {
    /// Identifier of the session whose dispute was resolved.
    pub session_id: Bytes,
    /// Address of the admin who resolved the dispute.
    pub resolver: Address,
    /// Amount returned to the buyer (payer).
    pub buyer_share: i128,
    /// Amount released to the seller (payee).
    pub seller_share: i128,
    /// Platform fee collected by the treasury.
    pub fee: i128,
    /// Ledger timestamp at the moment of resolution.
    pub timestamp: u64,
}

/// Emitted when the admin changes the treasury wallet (issue #152).
///
/// Emitted during `set_treasury()`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TreasuryUpdated {
    /// Previous treasury address.
    pub old_treasury: Address,
    /// New treasury address.
    pub new_treasury: Address,
    /// Address of the admin who performed the update.
    pub updated_by: Address,
}

/// Emitted when the contract WASM is upgraded (issue #153).
///
/// Allows off-chain monitoring of contract upgrades.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractUpgraded {
    /// Hash of the previous WASM code.
    pub old_wasm_hash: Bytes,
    /// Hash of the new WASM code.
    pub new_wasm_hash: Bytes,
    /// Address of the admin who triggered the upgrade.
    pub upgraded_by: Address,
    /// Ledger timestamp at the moment of the upgrade.
    pub timestamp: u64,
}

/// Emitted when a session is approved using off-chain signatures.
/// Closes issue #xxx.
#[contracttype]
#[derive(Clone, Debug)]
pub struct OffchainApprovalExecuted {
    /// Identifier of the session that was approved.
    pub session_id: Bytes,
    /// Address of the buyer who signed.
    pub buyer: Address,
    /// Address of the seller who signed.
    pub seller: Address,
    /// Amount paid to the seller.
    pub payout: i128,
    /// Platform fee collected.
    pub fee: i128,
    /// Ledger timestamp at the moment of approval.
    pub timestamp: u64,
}
