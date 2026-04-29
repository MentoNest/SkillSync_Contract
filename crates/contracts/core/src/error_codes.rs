use soroban_sdk::contracterror;

/// Errors for contract initialization failures (issue #155).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InitError {
    /// Contract has already been initialized.
    AlreadyInitialized = 100,
    /// Contract has not been initialized yet.
    NotInitialized = 101,
    /// Admin address is zero or invalid.
    InvalidAdmin = 102,
    /// Treasury address is zero or invalid.
    InvalidTreasury = 103,
}

/// Errors for unauthorized function calls (issue #156).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AuthError {
    /// Generic unauthorized access.
    Unauthorized = 200,
    /// Caller is not the contract admin.
    NotAdmin = 201,
    /// Caller is not the session buyer.
    NotBuyer = 202,
    /// Caller is not the session seller.
    NotSeller = 203,
}

/// Errors for invalid session operations (issue #157).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SessionError {
    /// Session ID does not exist.
    SessionNotFound = 300,
    /// Session ID already exists.
    DuplicateSessionId = 301,
    /// Operation not allowed in the current session state.
    InvalidSessionState = 302,
    /// Cannot complete a session that is already completed.
    SessionAlreadyCompleted = 303,
    /// Cannot approve a session that is already approved.
    SessionAlreadyApproved = 304,
    /// Session has already been refunded.
    SessionAlreadyRefunded = 305,
    /// Cannot act on a session that is currently in dispute.
    SessionInDispute = 306,
}

/// Errors for amount and fee validation (issue #158).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FinancialError {
    /// Amount is zero or negative.
    InvalidAmount = 400,
    /// Buyer has insufficient funds.
    InsufficientBalance = 401,
    /// Fee exceeds the maximum allowed (1000 bps).
    FeeTooHigh = 402,
    /// Dispute split does not sum to the session amount.
    InvalidSplit = 403,
    /// Arithmetic overflow detected.
    Overflow = 404,
}

/// Errors for timeout and dispute operations (issue #159).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TimeoutDisputeError {
    /// Cannot auto-refund yet — dispute window has not elapsed.
    DisputeWindowNotElapsed = 500,
    /// Dispute already exists for this session.
    DisputeAlreadyOpen = 501,
    /// No open dispute to resolve on this session.
    DisputeNotOpen = 502,
    /// Session is not eligible for resolution.
    ResolutionNotAllowed = 503,
}

/// Errors for contract upgrade failures (issue #160).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum UpgradeError {
    /// Provided WASM hash is zero or invalid.
    InvalidWasmHash = 600,
    /// Low-level upgrade failure.
    UpgradeFailed = 601,
}

/// Reentrancy protection error (issue #209).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReentrancyError {
    /// A reentrant call was detected. Code 700 per spec.
    ReentrancyDetected = 700,
}
