/// Storage cleanup and archiving — issue #215
///
/// Prevents unbounded storage growth by moving finalised sessions to a
/// compact archive record and eventually deleting them.
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env};

use crate::{DataKey, Error, SessionStatus, SkillSyncContract, SECONDS_PER_DAY};

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub enum ArchiveKey {
    /// Compact archive record for a session.
    Archived(Bytes),
    /// Admin-configured ledgers-after-finalisation before archiving.
    ArchiveAfterLedgers,
}

/// Minimal data kept for an archived session.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ArchivedSession {
    /// Keccak-like hash of the original session (ledger sequence + session_id bytes).
    pub original_hash: Bytes,
    /// Who the payer was.
    pub payer: Address,
    /// Final status at the time of archiving.
    pub final_status: SessionStatus,
    /// Ledger timestamp when archived.
    pub archived_at: u64,
}

/// Default: archive sessions finalised more than 30 days ago.
pub const DEFAULT_ARCHIVE_AFTER_SECONDS: u64 = 30 * SECONDS_PER_DAY;

// ── Events ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct SessionArchivedEvent {
    pub session_id: Bytes,
    pub archived_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct SessionDeletedEvent {
    pub session_id: Bytes,
    pub deleted_at: u64,
}

// ── Implementation ────────────────────────────────────────────────────────────

impl SkillSyncContract {
    /// Admin: set how many seconds after finalisation a session is eligible for archiving.
    pub fn set_archive_after_seconds(env: Env, seconds: u64) -> Result<(), Error> {
        let admin = crate::read_admin(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&ArchiveKey::ArchiveAfterLedgers, &seconds);
        Ok(())
    }

    fn archive_after_seconds(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&ArchiveKey::ArchiveAfterLedgers)
            .unwrap_or(DEFAULT_ARCHIVE_AFTER_SECONDS)
    }

    fn is_finalised(status: &SessionStatus) -> bool {
        matches!(
            status,
            SessionStatus::Approved
                | SessionStatus::Refunded
                | SessionStatus::Resolved
                | SessionStatus::Cancelled
        )
    }

    /// Move a single finalised session from persistent storage to the compact archive.
    pub fn archive_session(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let admin = crate::read_admin(&env)?;
        admin.require_auth();

        let session =
            Self::get_session(env.clone(), session_id.clone()).ok_or(Error::SessionNotFound)?;

        if !Self::is_finalised(&session.status) {
            return Err(Error::InvalidSessionStatus);
        }

        let now = env.ledger().timestamp();
        let threshold = Self::archive_after_seconds(&env);
        if now < session.updated_at.saturating_add(threshold) {
            return Err(Error::DisputeWindowNotElapsed);
        }

        // Build a compact hash: XOR of session_id bytes with updated_at bytes.
        let mut hash_bytes = [0u8; 8];
        let ts_bytes = session.updated_at.to_be_bytes();
        let id_slice = session.session_id.slice(0..session.session_id.len().min(8));
        for i in 0..8usize {
            let id_byte = if (i as u32) < id_slice.len() {
                id_slice.get(i as u32).unwrap_or(0)
            } else {
                0
            };
            hash_bytes[i] = id_byte ^ ts_bytes[i];
        }
        let original_hash = Bytes::from_slice(&env, &hash_bytes);

        let archive = ArchivedSession {
            original_hash,
            payer: session.payer.clone(),
            final_status: session.status.clone(),
            archived_at: now,
        };

        env.storage()
            .persistent()
            .set(&ArchiveKey::Archived(session_id.clone()), &archive);

        // Remove the full session record.
        env.storage()
            .persistent()
            .remove(&DataKey::Session(session_id.clone()));

        env.events().publish(
            (symbol_short!("archived"),),
            SessionArchivedEvent {
                session_id,
                archived_at: now,
            },
        );

        Ok(())
    }

    /// Permanently delete an archived session after the archive period.
    pub fn delete_archived_session(env: Env, session_id: Bytes) -> Result<(), Error> {
        Self::require_not_paused(&env)?;
        let admin = crate::read_admin(&env)?;
        admin.require_auth();

        let archive: ArchivedSession = env
            .storage()
            .persistent()
            .get(&ArchiveKey::Archived(session_id.clone()))
            .ok_or(Error::SessionNotFound)?;

        let now = env.ledger().timestamp();
        let threshold = Self::archive_after_seconds(&env);
        if now < archive.archived_at.saturating_add(threshold) {
            return Err(Error::DisputeWindowNotElapsed);
        }

        env.storage()
            .persistent()
            .remove(&ArchiveKey::Archived(session_id.clone()));

        env.events().publish(
            (symbol_short!("deleted"),),
            SessionDeletedEvent {
                session_id,
                deleted_at: now,
            },
        );

        Ok(())
    }

    /// Gas-limited batch archive of up to `limit` eligible sessions from the
    /// expiry index.
    pub fn batch_archive_sessions(env: Env, limit: u32) -> Result<u32, Error> {
        Self::require_not_paused(&env)?;
        let admin = crate::read_admin(&env)?;
        admin.require_auth();

        let now = env.ledger().timestamp();
        let threshold = Self::archive_after_seconds(&env);
        let cutoff_ts = now.saturating_sub(threshold);
        let cutoff_bucket = cutoff_ts / SECONDS_PER_DAY;

        let last_processed: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LastProcessedExpiryBucket)
            .unwrap_or(0);

        let mut archived_count: u32 = 0;
        let mut bucket = last_processed;

        'outer: while bucket <= cutoff_bucket && archived_count < limit {
            let key = DataKey::ExpiryIndex(bucket);
            if let Some(session_ids) = env
                .storage()
                .persistent()
                .get::<_, soroban_sdk::Vec<Bytes>>(&key)
            {
                for i in 0..session_ids.len() {
                    if archived_count >= limit {
                        break 'outer;
                    }
                    let sid = session_ids.get(i).unwrap();
                    // Best-effort: skip sessions that can't be archived yet.
                    let _ = Self::archive_session(env.clone(), sid);
                    archived_count += 1;
                }
            }
            bucket += 1;
        }

        env.storage()
            .instance()
            .set(&DataKey::LastProcessedExpiryBucket, &bucket);

        Ok(archived_count)
    }

    /// Read an archived session record.
    pub fn get_archived_session(env: Env, session_id: Bytes) -> Option<ArchivedSession> {
        env.storage()
            .persistent()
            .get(&ArchiveKey::Archived(session_id))
    }
}
