use starknet::ContractAddress;

#[starknet::interface]
pub trait ITicketNFT<TContractState> {
    fn mint_ticket(ref self: TContractState, user: ContractAddress, event_id: felt252) -> u256;
    fn burn_ticket(ref self: TContractState, user: ContractAddress, event_id: u256);
    fn transfer_ticket(ref self: TContractState, from: ContractAddress, to: ContractAddress, token_id: u256);
}


#[starknet::contract]
mod TicketNFT {
    use openzeppelin_introspection::src5::SRC5Component;
    use openzeppelin_token::erc721::{ERC721Component, ERC721HooksEmptyImpl};
    use starknet::ContractAddress;
    use starknet::storage::{Map, StorageMapEntry, StoragePointerReadAccess, StoragePointerWriteAccess};

    component!(path: ERC721Component, storage: erc721, event: ERC721Event);
    component!(path: SRC5Component, storage: src5, event: SRC5Event);

    // ERC721 Mixin
    #[abi(embed_v0)]
    impl ERC721MixinImpl = ERC721Component::ERC721MixinImpl<ContractState>;
    impl ERC721InternalImpl = ERC721Component::InternalImpl<ContractState>;

    #[storage]
    struct Storage {
        user_event_ticket: Map<(ContractAddress, felt252), u256>, // Maps (user, event_id) to token_id
        token_id_counter: u256,
        #[substorage(v0)]
        erc721: ERC721Component::Storage,
        #[substorage(v0)]
        src5: SRC5Component::Storage
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    enum Event {
        TicketMinted: TicketMinted,
        TicketBurned: TicketBurned,
        TicketTransferred: TicketTransferred,
        #[flat]
        ERC721Event: ERC721Component::Event,
        #[flat]
        SRC5Event: SRC5Component::Event
    }

    #[derive(Drop, starknet::Event)]
    struct TicketMinted {
        user: ContractAddress,
        event_id: felt252,
        token_id: u256
    }
    #[derive(Drop, starknet::Event)]
    struct TicketBurned {
        token_id: u256
    }
    #[derive(Drop, starknet::Event)]
    struct TicketTransferred {
        from: ContractAddress,
        to: ContractAddress,
        token_id: u256
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        recipient: ContractAddress
    ) {
        let name = "TicketNFT";
        let symbol = "TicketNFT";
        let base_uri = "https://api.example.com/v1/";

        self.erc721.initializer(name, symbol, base_uri);
    }

    #[abi(embed_v0)]
    impl TicketImpl for ITicketNFT<ContractState> {
        fn mint_ticket(ref self: ContractState, user: ContractAddress, event_id: felt252) -> u256 {
            // Check if the user already has a ticket for this event
            let user_ticket = self.user_event_ticket.entry((user, event_id)).read();
            assert(user_ticket == 0, 'ticket for this event already minted');
            let token_id = self.token_id_counter.read() + 1;
            // Mint the ticket NFT
            let token_id = self.erc721.mint(user, token_id);
            // Add the mapping from user and event_id to token_id
            self.user_event_ticket.entry((user, event_id)).write(token_id);
            self.token_id_counter.write(token_id);
            // emit the event
            self.emit(
                TicketMinted {
                    user,
                    token_id,
                    event_id
                }
            )
            token_id
        }

        fn burn_ticket(ref self: ContractState, user: ContractAddress, event_id: u256) {
            // Get the token_id for the user and event_id
            let token_id = self.user_event_ticket.entry((user, event_id)).read();
            assert(token_id != 0, 'no ticket found for this user and event');
            // Burn the ticket NFT
            self.erc721.burn(token_id);
            // Remove the mapping from user and event_id to token_id
            self.user_event_ticket.entry((user, event_id)).write(0);
            // Emit the event before burning
            self.emit(
                TicketBurned {
                    token_id
                }
            );
        }

        fn transfer_ticket(
            ref self: ContractState,
            from: ContractAddress,
            to: ContractAddress,
            token_id: u256
        ) {
            // Transfer the ticket NFT
            self.erc721.transfer(from, to, token_id);
            // Update the user_event_ticket mapping
            let event_id = self.user_event_ticket.entry((from, token_id)).read();
            self.user_event_ticket.entry((from, event_id)).write(0); // Remove old mapping
            self.user_event_ticket.entry((to, event_id)).write(token_id); // Add new mapping
        }
    }
}