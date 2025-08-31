use starknet::ContractAddress;

#[starknet::interface]
trait ITicketNFT<TContractState> {
    fn create_ticket(ref self: TContractState, title: felt252, description: felt252, payment_amount: u256, deadline: u64) -> u256;
    fn assign_ticket(ref self: TContractState, ticket_id: u256, freelancer: ContractAddress);
    fn start_work(ref self: TContractState, ticket_id: u256);
    fn submit_work(ref self: TContractState, ticket_id: u256, submission_uri: felt252);
    fn approve_work(ref self: TContractState, ticket_id: u256);
    fn reject_work(ref self: TContractState, ticket_id: u256, reason: felt252);
    fn dispute_ticket(ref self: TContractState, ticket_id: u256, reason: felt252);
    fn resolve_dispute(ref self: TContractState, ticket_id: u256, resolution: u8); // 0: favor client, 1: favor freelancer
    fn cancel_ticket(ref self: TContractState, ticket_id: u256);
    fn withdraw_payment(ref self: TContractState, ticket_id: u256);
    fn get_ticket_details(self: @TContractState, ticket_id: u256) -> TicketDetails;
    fn get_user_tickets(self: @TContractState, user: ContractAddress, role: u8) -> Array<u256>; // 0: client, 1: freelancer
    fn get_total_tickets(self: @TContractState) -> u256;
    fn owner_of(self: @TContractState, ticket_id: u256) -> ContractAddress;
}

#[derive(Drop, Serde, starknet::Store)]
struct TicketDetails {
    ticket_id: u256,
    client: ContractAddress,
    freelancer: ContractAddress,
    title: felt252,
    description: felt252,
    payment_amount: u256,
    deadline: u64,
    created_timestamp: u64,
    status: TicketStatus,
    submission_uri: felt252,
    dispute_reason: felt252,
}

#[derive(Drop, Serde, starknet::Store)]
enum TicketStatus {
    Open,
    Assigned,
    InProgress,
    Submitted,
    Completed,
    Disputed,
    Cancelled,
    Resolved,
}

#[starknet::contract]
mod TicketNFT {
    use super::{ITicketNFT, TicketDetails, TicketStatus};
    use starknet::{ContractAddress, get_caller_address, get_block_timestamp};
    use starknet::storage::{
        StoragePointerReadAccess, StoragePointerWriteAccess, StoragePathEntry, Map, Vec, VecTrait, MutableVecTrait
    };

    #[storage]
    struct Storage {
        // NFT core storage
        owners: Map<u256, ContractAddress>, // ticket_id -> client (owner)
        
        // Ticket specific storage
        tickets: Map<u256, TicketDetails>,
        client_tickets: Map<ContractAddress, Vec<u256>>,
        freelancer_tickets: Map<ContractAddress, Vec<u256>>,
        ticket_counter: u256,
        
        // Payment escrow
        escrowed_payments: Map<u256, u256>, // ticket_id -> amount
        
        // Access control
        owner: ContractAddress,
        dispute_resolver: ContractAddress,
        
        // Platform fee (in basis points, e.g., 250 = 2.5%)
        platform_fee_bps: u256,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        TicketCreated: TicketCreated,
        TicketAssigned: TicketAssigned,
        WorkStarted: WorkStarted,
        WorkSubmitted: WorkSubmitted,
        WorkApproved: WorkApproved,
        WorkRejected: WorkRejected,
        TicketDisputed: TicketDisputed,
        DisputeResolved: DisputeResolved,
        TicketCancelled: TicketCancelled,
        PaymentWithdrawn: PaymentWithdrawn,
    }

    #[derive(Drop, starknet::Event)]
    struct TicketCreated {
        ticket_id: u256,
        client: ContractAddress,
        title: felt252,
        payment_amount: u256,
        deadline: u64,
    }

    #[derive(Drop, starknet::Event)]
    struct TicketAssigned {
        ticket_id: u256,
        client: ContractAddress,
        freelancer: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct WorkStarted {
        ticket_id: u256,
        freelancer: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct WorkSubmitted {
        ticket_id: u256,
        freelancer: ContractAddress,
        submission_uri: felt252,
    }

    #[derive(Drop, starknet::Event)]
    struct WorkApproved {
        ticket_id: u256,
        client: ContractAddress,
        freelancer: ContractAddress,
        payment_amount: u256,
    }

    #[derive(Drop, starknet::Event)]
    struct WorkRejected {
        ticket_id: u256,
        client: ContractAddress,
        reason: felt252,
    }

    #[derive(Drop, starknet::Event)]
    struct TicketDisputed {
        ticket_id: u256,
        disputer: ContractAddress,
        reason: felt252,
    }

    #[derive(Drop, starknet::Event)]
    struct DisputeResolved {
        ticket_id: u256,
        resolver: ContractAddress,
        resolution: u8,
        winner: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct TicketCancelled {
        ticket_id: u256,
        client: ContractAddress,
    }

    #[derive(Drop, starknet::Event)]
    struct PaymentWithdrawn {
        ticket_id: u256,
        recipient: ContractAddress,
        amount: u256,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress, dispute_resolver: ContractAddress) {
        self.owner.write(owner);
        self.dispute_resolver.write(dispute_resolver);
        self.platform_fee_bps.write(250); // 2.5% platform fee
    }

    #[abi(embed_v0)]
    impl TicketNFTImpl of ITicketNFT<ContractState> {
        fn create_ticket(ref self: ContractState, title: felt252, description: felt252, payment_amount: u256, deadline: u64) -> u256 {
            let caller = get_caller_address();
            let current_time = get_block_timestamp();
            
            assert(payment_amount > 0, 'Payment amount must be > 0');
            assert(deadline > current_time, 'Deadline must be in future');
            
            let ticket_id = self.ticket_counter.read() + 1;
            self.ticket_counter.write(ticket_id);
            
            let ticket = TicketDetails {
                ticket_id,
                client: caller,
                freelancer: Zeroable::zero(),
                title,
                description,
                payment_amount,
                deadline,
                created_timestamp: current_time,
                status: TicketStatus::Open,
                submission_uri: 0,
                dispute_reason: 0,
            };
            
            // Store ticket
            self.tickets.entry(ticket_id).write(ticket);
            self.owners.entry(ticket_id).write(caller);
            
            // Add to client's tickets
            let mut client_tickets = self.client_tickets.entry(caller).read();
            client_tickets.append().write(ticket_id);
            self.client_tickets.entry(caller).write(client_tickets);
            
            // Escrow payment (in real implementation, this would transfer tokens)
            self.escrowed_payments.entry(ticket_id).write(payment_amount);
            
            self.emit(TicketCreated { ticket_id, client: caller, title, payment_amount, deadline });
            
            ticket_id
        }

        fn assign_ticket(ref self: ContractState, ticket_id: u256, freelancer: ContractAddress) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.client == caller, 'Only client can assign');
            assert(matches!(ticket.status, TicketStatus::Open), 'Ticket must be open');
            assert(!freelancer.is_zero(), 'Invalid freelancer address');
            
            ticket.freelancer = freelancer;
            ticket.status = TicketStatus::Assigned;
            self.tickets.entry(ticket_id).write(ticket);
            
            // Add to freelancer's tickets
            let mut freelancer_tickets = self.freelancer_tickets.entry(freelancer).read();
            freelancer_tickets.append().write(ticket_id);
            self.freelancer_tickets.entry(freelancer).write(freelancer_tickets);
            
            self.emit(TicketAssigned { ticket_id, client: caller, freelancer });
        }

        fn start_work(ref self: ContractState, ticket_id: u256) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.freelancer == caller, 'Only assigned freelancer');
            assert(matches!(ticket.status, TicketStatus::Assigned), 'Ticket must be assigned');
            assert(get_block_timestamp() <= ticket.deadline, 'Deadline passed');
            
            ticket.status = TicketStatus::InProgress;
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(WorkStarted { ticket_id, freelancer: caller });
        }

        fn submit_work(ref self: ContractState, ticket_id: u256, submission_uri: felt252) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.freelancer == caller, 'Only assigned freelancer');
            assert(matches!(ticket.status, TicketStatus::InProgress), 'Work must be in progress');
            
            ticket.status = TicketStatus::Submitted;
            ticket.submission_uri = submission_uri;
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(WorkSubmitted { ticket_id, freelancer: caller, submission_uri });
        }

        fn approve_work(ref self: ContractState, ticket_id: u256) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.client == caller, 'Only client can approve');
            assert(matches!(ticket.status, TicketStatus::Submitted), 'Work must be submitted');
            
            ticket.status = TicketStatus::Completed;
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(WorkApproved { 
                ticket_id, 
                client: caller, 
                freelancer: ticket.freelancer, 
                payment_amount: ticket.payment_amount 
            });
        }

        fn reject_work(ref self: ContractState, ticket_id: u256, reason: felt252) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.client == caller, 'Only client can reject');
            assert(matches!(ticket.status, TicketStatus::Submitted), 'Work must be submitted');
            
            ticket.status = TicketStatus::InProgress; // Back to in progress
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(WorkRejected { ticket_id, client: caller, reason });
        }

        fn dispute_ticket(ref self: ContractState, ticket_id: u256, reason: felt252) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(caller == ticket.client || caller == ticket.freelancer, 'Only parties can dispute');
            assert(!matches!(ticket.status, TicketStatus::Open) && !matches!(ticket.status, TicketStatus::Cancelled), 'Invalid status for dispute');
            
            ticket.status = TicketStatus::Disputed;
            ticket.dispute_reason = reason;
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(TicketDisputed { ticket_id, disputer: caller, reason });
        }

        fn resolve_dispute(ref self: ContractState, ticket_id: u256, resolution: u8) {
            let caller = get_caller_address();
            assert(caller == self.dispute_resolver.read() || caller == self.owner.read(), 'Only dispute resolver');
            
            let mut ticket = self.tickets.entry(ticket_id).read();
            assert(matches!(ticket.status, TicketStatus::Disputed), 'Ticket must be disputed');
            assert(resolution <= 1, 'Invalid resolution');
            
            ticket.status = TicketStatus::Resolved;
            self.tickets.entry(ticket_id).write(ticket);
            
            let winner = if resolution == 0 { ticket.client } else { ticket.freelancer };
            
            self.emit(DisputeResolved { ticket_id, resolver: caller, resolution, winner });
        }

        fn cancel_ticket(ref self: ContractState, ticket_id: u256) {
            let caller = get_caller_address();
            let mut ticket = self.tickets.entry(ticket_id).read();
            
            assert(ticket.client == caller, 'Only client can cancel');
            assert(matches!(ticket.status, TicketStatus::Open) || matches!(ticket.status, TicketStatus::Assigned), 'Cannot cancel at this stage');
            
            ticket.status = TicketStatus::Cancelled;
            self.tickets.entry(ticket_id).write(ticket);
            
            self.emit(TicketCancelled { ticket_id, client: caller });
        }

        fn withdraw_payment(ref self: ContractState, ticket_id: u256) {
            let caller = get_caller_address();
            let ticket = self.tickets.entry(ticket_id).read();
            let escrowed_amount = self.escrowed_payments.entry(ticket_id).read();
            
            assert(escrowed_amount > 0, 'No payment to withdraw');
            
            let can_withdraw = match ticket.status {
                TicketStatus::Completed => caller == ticket.freelancer,
                TicketStatus::Cancelled => caller == ticket.client,
                TicketStatus::Resolved => {
                    // In real implementation, check resolution details
                    caller == ticket.client || caller == ticket.freelancer
                },
                _ => false,
            };
            
            assert(can_withdraw, 'Cannot withdraw at this time');
            
            // Clear escrow
            self.escrowed_payments.entry(ticket_id).write(0);
            
            // In real implementation, transfer tokens to caller
            
            self.emit(PaymentWithdrawn { ticket_id, recipient: caller, amount: escrowed_amount });
        }

        fn get_ticket_details(self: @ContractState, ticket_id: u256) -> TicketDetails {
            self.tickets.entry(ticket_id).read()
        }

        fn get_user_tickets(self: @ContractState, user: ContractAddress, role: u8) -> Array<u256> {
            let user_tickets = if role == 0 {
                self.client_tickets.entry(user).read()
            } else {
                self.freelancer_tickets.entry(user).read()
            };
            
            let mut result = ArrayTrait::new();
            let mut i = 0;
            
            loop {
                if i >= user_tickets.len() {
                    break;
                }
                result.append(user_tickets.at(i).read());
                i += 1;
            };
            
            result
        }

        fn get_total_tickets(self: @ContractState) -> u256 {
            self.ticket_counter.read()
        }

        fn owner_of(self: @ContractState, ticket_id: u256) -> ContractAddress {
            let owner = self.owners.entry(ticket_id).read();
            assert(!owner.is_zero(), 'Ticket does not exist');
            owner
        }
    }
}
