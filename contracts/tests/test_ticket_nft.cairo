use snforge_std::{declare, ContractClassTrait, DeclareResultTrait, start_cheat_caller_address, stop_cheat_caller_address, start_cheat_block_timestamp, stop_cheat_block_timestamp};
use starknet::{ContractAddress, contract_address_const};
use skillsync_contracts::{ITicketNFTDispatcher, ITicketNFTDispatcherTrait, TicketDetails, TicketStatus};

fn OWNER() -> ContractAddress {
    contract_address_const::<'owner'>()
}

fn DISPUTE_RESOLVER() -> ContractAddress {
    contract_address_const::<'dispute_resolver'>()
}

fn CLIENT() -> ContractAddress {
    contract_address_const::<'client'>()
}

fn FREELANCER() -> ContractAddress {
    contract_address_const::<'freelancer'>()
}

fn OTHER_USER() -> ContractAddress {
    contract_address_const::<'other_user'>()
}

fn deploy_ticket_nft() -> ITicketNFTDispatcher {
    let contract = declare("TicketNFT").unwrap().contract_class();
    let (contract_address, _) = contract.deploy(@array![OWNER().into(), DISPUTE_RESOLVER().into()]).unwrap();
    ITicketNFTDispatcher { contract_address }
}

#[test]
fn test_create_ticket() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 2000);
    
    assert(ticket_id == 1, 'Ticket ID should be 1');
    assert(ticket_nft.owner_of(ticket_id) == CLIENT(), 'Client should own ticket');
    assert(ticket_nft.get_total_tickets() == 1, 'Total tickets should be 1');
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(details.client == CLIENT(), 'Client should match');
    assert(details.title == 'Web Development', 'Title should match');
    assert(details.description == 'Build a website', 'Description should match');
    assert(details.payment_amount == 1000, 'Payment amount should match');
    assert(details.deadline == 2000, 'Deadline should match');
    assert(details.created_timestamp == 1000, 'Created timestamp should match');
    assert(matches!(details.status, TicketStatus::Open), 'Status should be Open');
    
    stop_cheat_caller_address(ticket_nft.contract_address);
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Payment amount must be > 0',))]
fn test_create_ticket_zero_payment() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    ticket_nft.create_ticket('Web Development', 'Build a website', 0, 2000); // Should panic
    
    stop_cheat_caller_address(ticket_nft.contract_address);
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Deadline must be in future',))]
fn test_create_ticket_past_deadline() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 500); // Should panic
    
    stop_cheat_caller_address(ticket_nft.contract_address);
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
fn test_assign_ticket() {
    let ticket_nft = deploy_ticket_nft();
    
    // Create ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 2000);
    
    // Assign ticket
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(details.freelancer == FREELANCER(), 'Freelancer should be assigned');
    assert(matches!(details.status, TicketStatus::Assigned), 'Status should be Assigned');
    
    stop_cheat_caller_address(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Only client can assign',))]
fn test_unauthorized_assign() {
    let ticket_nft = deploy_ticket_nft();
    
    // Create ticket as client
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 2000);
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Try to assign as other user
    start_cheat_caller_address(ticket_nft.contract_address, OTHER_USER());
    ticket_nft.assign_ticket(ticket_id, FREELANCER()); // Should panic
    stop_cheat_caller_address(ticket_nft.contract_address);
}

#[test]
fn test_full_workflow() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // 1. Create ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 3000);
    
    // 2. Assign ticket
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // 3. Start work
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.start_work(ticket_id);
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::InProgress), 'Status should be InProgress');
    
    // 4. Submit work
    ticket_nft.submit_work(ticket_id, 'submission_uri_123');
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::Submitted), 'Status should be Submitted');
    assert(details.submission_uri == 'submission_uri_123', 'Submission URI should match');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // 5. Approve work
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    ticket_nft.approve_work(ticket_id);
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::Completed), 'Status should be Completed');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // 6. Withdraw payment
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.withdraw_payment(ticket_id);
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
fn test_reject_and_resubmit() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // Create and assign ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 3000);
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Start work and submit
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.start_work(ticket_id);
    ticket_nft.submit_work(ticket_id, 'first_submission');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Reject work
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    ticket_nft.reject_work(ticket_id, 'Needs improvements');
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::InProgress), 'Status should be back to InProgress');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Resubmit work
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.submit_work(ticket_id, 'improved_submission');
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(details.submission_uri == 'improved_submission', 'Should have new submission URI');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
fn test_dispute_workflow() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // Create, assign, and submit work
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 3000);
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.start_work(ticket_id);
    ticket_nft.submit_work(ticket_id, 'submission');
    
    // Freelancer disputes
    ticket_nft.dispute_ticket(ticket_id, 'Client not responding');
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::Disputed), 'Status should be Disputed');
    assert(details.dispute_reason == 'Client not responding', 'Dispute reason should match');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Resolve dispute in favor of freelancer
    start_cheat_caller_address(ticket_nft.contract_address, DISPUTE_RESOLVER());
    ticket_nft.resolve_dispute(ticket_id, 1); // 1 = favor freelancer
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::Resolved), 'Status should be Resolved');
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
fn test_cancel_ticket() {
    let ticket_nft = deploy_ticket_nft();
    
    // Create ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 2000);
    
    // Cancel ticket
    ticket_nft.cancel_ticket(ticket_id);
    
    let details = ticket_nft.get_ticket_details(ticket_id);
    assert(matches!(details.status, TicketStatus::Cancelled), 'Status should be Cancelled');
    
    // Client should be able to withdraw payment
    ticket_nft.withdraw_payment(ticket_id);
    
    stop_cheat_caller_address(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Cannot cancel at this stage',))]
fn test_cannot_cancel_in_progress() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // Create, assign, and start work
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 3000);
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.start_work(ticket_id);
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Try to cancel - should fail
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    ticket_nft.cancel_ticket(ticket_id); // Should panic
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
fn test_get_user_tickets() {
    let ticket_nft = deploy_ticket_nft();
    
    // Create multiple tickets
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id1 = ticket_nft.create_ticket('Web Dev', 'Website', 1000, 2000);
    let ticket_id2 = ticket_nft.create_ticket('Mobile App', 'iOS App', 2000, 3000);
    
    // Assign one ticket
    ticket_nft.assign_ticket(ticket_id1, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Check client tickets
    let client_tickets = ticket_nft.get_user_tickets(CLIENT(), 0); // 0 = client role
    assert(client_tickets.len() == 2, 'Client should have 2 tickets');
    assert(*client_tickets.at(0) == ticket_id1, 'First ticket should match');
    assert(*client_tickets.at(1) == ticket_id2, 'Second ticket should match');
    
    // Check freelancer tickets
    let freelancer_tickets = ticket_nft.get_user_tickets(FREELANCER(), 1); // 1 = freelancer role
    assert(freelancer_tickets.len() == 1, 'Freelancer should have 1 ticket');
    assert(*freelancer_tickets.at(0) == ticket_id1, 'Assigned ticket should match');
}

#[test]
#[should_panic(expected: ('Only assigned freelancer',))]
fn test_unauthorized_start_work() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // Create and assign ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 3000);
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Try to start work as wrong user
    start_cheat_caller_address(ticket_nft.contract_address, OTHER_USER());
    ticket_nft.start_work(ticket_id); // Should panic
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Deadline passed',))]
fn test_start_work_after_deadline() {
    let ticket_nft = deploy_ticket_nft();
    
    start_cheat_block_timestamp(ticket_nft.contract_address, 1000);
    
    // Create and assign ticket with early deadline
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 1500);
    ticket_nft.assign_ticket(ticket_id, FREELANCER());
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    // Move time past deadline
    stop_cheat_block_timestamp(ticket_nft.contract_address);
    start_cheat_block_timestamp(ticket_nft.contract_address, 2000);
    
    // Try to start work after deadline
    start_cheat_caller_address(ticket_nft.contract_address, FREELANCER());
    ticket_nft.start_work(ticket_id); // Should panic
    stop_cheat_caller_address(ticket_nft.contract_address);
    
    stop_cheat_block_timestamp(ticket_nft.contract_address);
}

#[test]
#[should_panic(expected: ('Cannot withdraw at this time',))]
fn test_unauthorized_withdrawal() {
    let ticket_nft = deploy_ticket_nft();
    
    // Create ticket
    start_cheat_caller_address(ticket_nft.contract_address, CLIENT());
    let ticket_id = ticket_nft.create_ticket('Web Development', 'Build a website', 1000, 2000);
    
    // Try to withdraw while ticket is still open
    ticket_nft.withdraw_payment(ticket_id); // Should panic
    
    stop_cheat_caller_address(ticket_nft.contract_address);
}
