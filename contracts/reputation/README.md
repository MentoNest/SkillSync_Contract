# Reputation Scoring Contract

A secure ink! smart contract for SkillSync that maintains on-chain reputation scores for mentors and mentees, updated after session completion and reviews.

## Overview

The Reputation contract implements a simple reputation system:

- **Session Completion**: Both mentor and mentee receive a fixed score bonus
- **Reviews**: Admin-applied ratings adjust mentor scores
- **Query Scores**: Public access to reputation scores
- **Admin Control**: Only authorized admin can update scores

## Features

### Core Functionality

- **Session Bonuses**: Automatic score increases for completed sessions
- **Review Adjustments**: Flexible rating system for post-session feedback
- **Score Persistence**: On-chain storage with saturating arithmetic
- **Event Logging**: All score changes emit events for transparency

### Security

- **Admin Authorization**: All update operations require admin privileges
- **Overflow Protection**: Uses saturating arithmetic to prevent integer overflow
- **Immutable History**: Events provide complete audit trail

## Interface

### Constructor

```rust
fn new(admin: AccountId, session_bonus: i64) -> Self
```

Initializes the contract with an admin account and session completion bonus.

### Messages

#### `bump_after_session(mentor: AccountId, mentee: AccountId) -> Result<(), ReputationError>`

Increases reputation scores for both mentor and mentee after a completed session.

**Parameters:**
- `mentor`: The mentor's account ID
- `mentee`: The mentee's account ID

**Requirements:** Caller must be admin

**Events:** Emits `ReputationUpdated` for both accounts

#### `apply_review(mentor: AccountId, rating: i32) -> Result<(), ReputationError>`

Applies a review rating to the mentor's reputation score.

**Parameters:**
- `mentor`: The mentor's account ID
- `rating`: The review rating (can be positive or negative)

**Requirements:** Caller must be admin

**Events:** Emits `ReputationUpdated`

#### `get(addr: AccountId) -> i64`

Retrieves the current reputation score for an account.

**Parameters:**
- `addr`: The account ID to query

**Returns:** The reputation score (0 if not set)

#### `admin() -> AccountId`

Returns the admin account ID.

## Events

### `ReputationUpdated`

Emitted when a reputation score changes.

**Fields:**
- `addr`: The account whose score changed
- `new_score`: The updated score
- `reason`: The reason for the change ("session_completion" or "review")

## Error Handling

### `ReputationError::Unauthorized`

Returned when a non-admin attempts to update scores.

## Testing

The contract includes comprehensive unit tests covering:

- Score increments after sessions
- Review application with positive/negative ratings
- Authorization checks
- Overflow protection using saturating arithmetic

Run tests with:
```bash
cargo test
```

## Usage Example

```rust
// Deploy contract
let contract = Reputation::new(admin_account, 10);

// Complete a session
contract.bump_after_session(mentor, mentee)?;

// Apply a review
contract.apply_review(mentor, 5)?;

// Query scores
let mentor_score = contract.get(mentor);
let mentee_score = contract.get(mentee);
```