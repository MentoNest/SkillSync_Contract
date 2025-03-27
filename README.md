# SkillSync Smart Contract

A decentralized smart contract system for managing freelance work agreements and payments using Cairo on StarkNet.

## Prerequisites

Before you begin, ensure you have the following installed:
- Python 3.9 or higher
- [Cairo Lang](https://www.cairo-lang.org/docs/quickstart.html)
- [Starknet-devnet](https://github.com/Shard-Labs/starknet-devnet)
- pip (Python package manager)

## Installation

1. Install Cairo:
```bash
curl -L https://raw.githubusercontent.com/starkware-libs/cairo-lang/master/scripts/install.sh | bash
```

2. Install Python dependencies:
```bash
pip install cairo-lang starknet-devnet pytest pytest-asyncio
```

3. Set up your environment:
```bash
cp .env.example .env
# Edit .env with your configuration
```

## Project Structure

```
contracts/
├── src/
│   ├── modules/        # Modular contract components
│   └── main.cairo      # Main contract entry point
├── tests/
│   └── test_main.py    # Contract tests
├── README.md           # This file
└── .env.example        # Environment configuration template
```

## Development

### Running Tests

```bash
pytest contracts/tests/
```

### Local Development

1. Start local Starknet devnet:
```bash
starknet-devnet
```

2. Deploy contract (from project root):
```bash
starknet-compile contracts/src/main.cairo
starknet deploy --contract main_compiled.json
```

## Current Implementation

The current implementation includes:
- Basic contract structure
- "God bless Ezen-wata" placeholder message
- Test framework setup

## Future Implementation Areas

1. User Management
   - Registration
   - Profile management
   - Reputation system

2. Contract Management
   - Creation
   - Validation
   - State management

3. Payment System
   - Token integration
   - Payment processing
   - Escrow functionality

4. Dispute Resolution
   - Dispute initiation
   - Resolution mechanism
   - Voting system