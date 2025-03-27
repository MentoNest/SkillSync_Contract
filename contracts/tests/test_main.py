"""
Tests for the SkillSync main contract
"""
import pytest
from starkware.starknet.testing.starknet import Starknet

@pytest.mark.asyncio
async def test_greeting():
    # Deploy contract
    starknet = await Starknet.empty()
    contract = await starknet.deploy(
        "contracts/src/main.cairo"
    )

    # Call the get_greeting function
    response = await contract.get_greeting().call()
    
    # Check if the response matches expected value
    assert response.result == ('God bless Ezen-wata',)
