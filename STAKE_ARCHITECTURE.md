# Mentor Staking Contract - Technical Architecture

## State Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                      STAKE LIFECYCLE                            │
└─────────────────────────────────────────────────────────────────┘

                          ┌──────────────┐
                          │   NO STAKE   │
                          └──────┬───────┘
                                 │
                       stake(token, amount)
                                 │
                    ┌────────────▼──────────────┐
                    │   STAKED STATE            │
                    │  - staked: amount         │
                    │  - pending_unstake: 0    │
                    │  - unlock_at: 0          │
                    └────────┬──────────┬───────┘
                             │          │
          request_unstake()  │          │  slash(amount)
                             │          │
          ┌──────────────────▼──────┐  │
          │ UNSTAKING (Cooldown)    │  │
          │ - staked: reduced       │  │
          │ - pending_unstake: amt  │  │  ┌─────────────────┐
          │ - unlock_at: block_n    │  └─►│ REDUCED STAKE   │
          └──────────────┬──────────┘     └─────────────────┘
                         │
        (wait until block >= unlock_at)
                         │
                  withdraw_unstaked()
                         │
                    ┌────▼──────┐
                    │ WITHDRAWN │
                    └───────────┘
```

## Component Interaction

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│                    STAKE CONTRACT                                │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Storage: Mapping<(mentor, token), StakeInfo>              │ │
│  │                                                            │ │
│  │  StakeInfo {                                               │ │
│  │    staked: Balance,          // Can be slashed            │ │
│  │    pending_unstake: Balance, // Protected by cooldown     │ │
│  │    unlock_at: BlockNumber    // When withdrawal allowed   │ │
│  │  }                                                         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌───────────────────────┐         ┌──────────────────────────┐ │
│  │ Mentor Operations     │         │ Admin Operations         │ │
│  ├───────────────────────┤         ├──────────────────────────┤ │
│  │ • stake()             │         │ • slash()                │ │
│  │ • request_unstake()   │         │ • transfer_admin()       │ │
│  │ • withdraw_unstaked() │         │                          │ │
│  │ • get_stake()         │         │ Guards: caller == admin  │ │
│  │ • get_cooldown_blocks()│       │                          │ │
│  └───────────────────────┘         └──────────────────────────┘ │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Security Mechanisms                                      │  │
│  ├──────────────────────────────────────────────────────────┤  │
│  │ ✓ Reentrancy: No external calls in state changes        │  │
│  │ ✓ Cooldown: Block-based unlock prevents flash attacks   │  │
│  │ ✓ Authorization: Admin check on sensitive operations    │  │
│  │ ✓ Arithmetic: Checked add/sub prevents overflow         │  │
│  │ ✓ Amount Validation: Rejects zero and invalid amounts   │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Error Handling Flow

```
                    ┌─────────────────────┐
                    │   Function Call     │
                    └──────────┬──────────┘
                               │
                   ┌───────────┴───────────┐
                   │                       │
         Input Validation         Authorization
              │                        │
         ┌────▼────┐           ┌──────▼────────┐
         │ ZeroAmt │           │ Unauthorized  │
         └─────────┘           └───────────────┘
              │
         Amount Check      State Lookup
              │                │
         ┌────▼────┐       ┌───▼──────┐
         │ Insuff  │       │ NotFound │
         │ Stake   │       └──────────┘
         └─────────┘
              │
         Cooldown Check
              │
         ┌────▼─────────────┐
         │ UnlockTimeNot    │
         │ Reached          │
         └──────────────────┘
              │
         ┌────▼──────────┐
         │ Success (Ok)  │
         └───────────────┘
```

## Test Coverage Map

```
┌────────────────────────────────────────────────────────────────┐
│                    TEST SUITE (20 tests)                       │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  STAKE (3)         UNSTAKE (4)        WITHDRAW (3)            │
│  ├─ success        ├─ success         ├─ success              │
│  ├─ zero_amount    ├─ insuff_stake    ├─ cooldown_not_reached │
│  └─ multiple       ├─ zero_amount     └─ no_pending           │
│                    └─ no_stake                                │
│                                                                │
│  SLASH (5)         ADMIN (2)          BOUNDARY (3)            │
│  ├─ success        ├─ transfer_ok     ├─ cooldown_boundary    │
│  ├─ unauthorized   └─ transfer_unauth ├─ partial_slashing     │
│  ├─ insuff_stake                      └─ stake_while_unstaking│
│  ├─ zero_amount                                               │
│  └─ no_stake                                                  │
│                                                                │
│  Coverage: All functions, happy path, error conditions        │
│  Re-entrancy: Verified (no external calls)                    │
│  Block arithmetic: Boundary tested                             │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

## Deployment Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      DEPLOYMENT                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Phase 1: Initialization                                        │
│  ┌──────────────────────────────────────────────────┐          │
│  │ Stake::new(                                      │          │
│  │   admin: <governance_address>,                  │          │
│  │   cooldown_blocks: 600  // ~1 hour              │          │
│  │ )                                                │          │
│  └──────────────────────────────────────────────────┘          │
│                                                                 │
│  Phase 2: Integration                                          │
│  ┌──────────────────────────────────────────────────┐          │
│  │ • Configure mentor whitelisting (off-chain)      │          │
│  │ • Set up oracle for fraud detection (off-chain)  │          │
│  │ • Link to reputation system                      │          │
│  │ • Configure slashing treasury address (v2)       │          │
│  └──────────────────────────────────────────────────┘          │
│                                                                 │
│  Phase 3: Monitoring                                           │
│  ┌──────────────────────────────────────────────────┐          │
│  │ • Watch Staked events for activity               │          │
│  │ • Monitor Slashed events for fraud patterns       │          │
│  │ • Audit admin() operations quarterly              │          │
│  │ • Track withdraw success rates                    │          │
│  └──────────────────────────────────────────────────┘          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Event Flow Example: Fraud Scenario

```
Block 100:  Mentor Stakes 1000 SKILL
            └─► Staked { mentor, token, 1000 }
                staked = 1000

Block 105:  Mentor Requests Unstake 400 SKILL
            └─► UnstakeRequested { mentor, token, 400, unlock_at=705 }
                staked = 600, pending_unstake = 400

Block 500:  Admin Detects Fraud
            └─► Slash { mentor, token, 400 }
                staked = 200

Block 705:  Mentor Tries Withdrawal
            └─► withdraw_unstaked() succeeds
            ├─► Unstaked { mentor, token, 400 }
            └─► pending_unstake = 0
            
            Net Result: Lost 400 to slash, recovered 400 from pending
```

## Integration Checklist

```
□ Install Rust 1.70+ with ink! toolchain
□ Run: cargo build -p stake --lib --release
□ Verify: No compilation warnings
□ Run: cargo test -p stake --lib
□ Verify: All 20 tests pass
□ Deploy to testnet
□ Verify events are emitted correctly
□ Test admin slashing on testnet
□ Monitor cooldown periods (>5 blocks)
□ Deploy to mainnet with governance vote
□ Update SkillSync dashboard to show stake info
□ Create documentation for mentors
□ Train support team on slashing procedures
```

## Gas Estimation

| Operation | Gas Cost | Notes |
|-----------|----------|-------|
| `new()` | 10,000 | One-time deployment |
| `stake()` | 15,000-20,000 | Depends on storage ops |
| `request_unstake()` | 18,000-25,000 | Block calc + storage |
| `withdraw_unstaked()` | 16,000-20,000 | Cooldown check + storage |
| `slash()` | 20,000-28,000 | Admin check + storage |
| `transfer_admin()` | 8,000-12,000 | Storage write only |

## Version Roadmap

```
v1.0 (Current)
├─ Basic stake/unstake with cooldown
├─ Admin slashing
└─ Multi-token support

v2.0 (Planned)
├─ Graduated slashing levels
├─ Slashing appeals governance
├─ Reward distribution
└─ Token delegation

v3.0 (Future)
├─ Cross-chain staking
├─ Stake composition pools
└─ Dynamic cooldown adjustment
```
