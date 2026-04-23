use soroban_sdk::contracterror;

/// Standard contract error codes (0–255).
///
/// Each variant maps to a unique numeric code used across the SkillSync contract.
/// Implements `Into<u32>` via the `#[contracterror]` macro.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    // ── General (0–9) ────────────────────────────────────────────────────────
    /// No error / success sentinel (unused at runtime, reserved as 0).
    None = 0,
    /// Contract has already been initialised.
    AlreadyInitialized = 1,
    /// Contract has not been initialised yet.
    NotInitialized = 2,
    /// Caller is not authorised to perform this action.
    Unauthorized = 3,
    /// The provided address is invalid or empty.
    InvalidAddress = 4,
    /// Arithmetic overflow or underflow detected.
    ArithmeticOverflow = 5,
    /// Reentrancy detected.
    Reentrancy = 6,
    /// Nonce has already been used (replay protection).
    NonceAlreadyUsed = 7,
    /// Generic internal error.
    InternalError = 8,
    /// Feature is not yet implemented.
    NotImplemented = 9,

    // ── Session (10–39) ───────────────────────────────────────────────────────
    /// A session with this ID already exists.
    DuplicateSessionId = 10,
    /// The requested session was not found.
    SessionNotFound = 11,
    /// The session is not in the expected status.
    InvalidSessionStatus = 12,
    /// The session has not yet expired.
    SessionNotExpired = 13,
    /// The session ID is empty or exceeds the maximum length.
    InvalidSessionId = 14,
    /// The session is not in a valid state for the requested operation.
    InvalidSessionState = 15,
    /// The session has already been completed.
    SessionAlreadyCompleted = 16,
    /// The session has already been cancelled.
    SessionAlreadyCancelled = 17,
    /// The session has already been refunded.
    SessionAlreadyRefunded = 18,
    /// The session has already been resolved.
    SessionAlreadyResolved = 19,
    /// The session has already been approved.
    AlreadyApproved = 20,
    /// The caller is not an authorised party for this session.
    NotAuthorizedParty = 21,
    /// The session has expired.
    SessionExpired = 22,
    /// The session version is incompatible.
    SessionVersionMismatch = 23,

    // ── Funds / Payments (40–69) ──────────────────────────────────────────────
    /// The provided amount is invalid (zero, negative, or exceeds maximum).
    InvalidAmount = 40,
    /// The payer has insufficient balance.
    InsufficientBalance = 41,
    /// A token transfer failed.
    TransferError = 42,
    /// The amount exceeds the maximum allowed value.
    AmountTooLarge = 43,
    /// Fee calculation resulted in overflow or underflow.
    FeeCalculationOverflow = 44,
    /// The fee basis-points value is invalid.
    InvalidFeeBps = 45,
    /// Refund operation failed.
    RefundFailed = 46,
    /// No expired sessions available to sweep.
    NothingToSweep = 47,
    /// The resolution amounts do not sum to the available amount.
    InvalidResolutionAmount = 48,
    /// Error calculating resolution fees.
    ResolutionFeeError = 49,
    /// Payer and payee must be different addresses.
    SamePayerAndPayee = 50,

    // ── Dispute (70–99) ───────────────────────────────────────────────────────
    /// The dispute window has not yet elapsed.
    DisputeWindowNotElapsed = 70,
    /// The dispute window period is invalid.
    InvalidDisputeWindow = 71,
    /// The session is not in the Disputed status.
    SessionNotDisputed = 72,
    /// The session is not in a valid state for opening a dispute.
    InvalidDisputeState = 73,
    /// A dispute has already been opened for this session.
    DisputeAlreadyOpened = 74,
    /// The dispute deadline has passed.
    DisputeDeadlinePassed = 75,

    // ── Upgrade (100–119) ─────────────────────────────────────────────────────
    /// No upgrade has been proposed.
    UpgradeNotProposed = 100,
    /// The upgrade timelock has not yet elapsed.
    UpgradeNotReady = 101,
    /// The upgrade deadline has passed.
    UpgradeDeadlinePassed = 102,
    /// The timelock duration is invalid.
    InvalidTimelock = 103,
    /// The WASM hash provided for the upgrade is invalid.
    InvalidWasmHash = 104,

    // ── Treasury (120–129) ────────────────────────────────────────────────────
    /// The treasury address is invalid.
    InvalidTreasuryAddress = 120,
    /// The treasury has not been set.
    TreasuryNotSet = 121,

    // ── Reputation (130–149) ──────────────────────────────────────────────────
    /// The rating value is invalid (must be 1–5).
    InvalidRating = 130,
    /// Reputation calculation overflow.
    ReputationOverflow = 131,

    // ── Input Validation (150–179) ────────────────────────────────────────────
    /// The resolution note exceeds the maximum allowed length.
    InvalidNote = 150,
    /// A required field is missing.
    MissingField = 151,
    /// The provided data is malformed.
    MalformedData = 152,

    // ── Access Control (180–199) ──────────────────────────────────────────────
    /// The caller is not the contract admin.
    NotAdmin = 180,
    /// The caller is not the contract owner.
    NotOwner = 181,
    /// The operation requires multi-sig approval.
    MultiSigRequired = 182,

    // ── Reserved / Future Use (200–255) ───────────────────────────────────────
    Reserved200 = 200,
    Reserved201 = 201,
    Reserved202 = 202,
    Reserved203 = 203,
    Reserved204 = 204,
    Reserved205 = 205,
    Reserved206 = 206,
    Reserved207 = 207,
    Reserved208 = 208,
    Reserved209 = 209,
    Reserved210 = 210,
    Reserved211 = 211,
    Reserved212 = 212,
    Reserved213 = 213,
    Reserved214 = 214,
    Reserved215 = 215,
    Reserved216 = 216,
    Reserved217 = 217,
    Reserved218 = 218,
    Reserved219 = 219,
    Reserved220 = 220,
    Reserved221 = 221,
    Reserved222 = 222,
    Reserved223 = 223,
    Reserved224 = 224,
    Reserved225 = 225,
    Reserved226 = 226,
    Reserved227 = 227,
    Reserved228 = 228,
    Reserved229 = 229,
    Reserved230 = 230,
    Reserved231 = 231,
    Reserved232 = 232,
    Reserved233 = 233,
    Reserved234 = 234,
    Reserved235 = 235,
    Reserved236 = 236,
    Reserved237 = 237,
    Reserved238 = 238,
    Reserved239 = 239,
    Reserved240 = 240,
    Reserved241 = 241,
    Reserved242 = 242,
    Reserved243 = 243,
    Reserved244 = 244,
    Reserved245 = 245,
    Reserved246 = 246,
    Reserved247 = 247,
    Reserved248 = 248,
    Reserved249 = 249,
    Reserved250 = 250,
    Reserved251 = 251,
    Reserved252 = 252,
    Reserved253 = 253,
    Reserved254 = 254,
    Reserved255 = 255,
}
