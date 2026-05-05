use alloy_primitives::{keccak256, Address, Bytes, B256, U256};
use std::collections::BTreeSet;

use revm::{
    bytecode::Bytecode,
    context::{Context, TxEnv},
    database::{CacheDB, EmptyDB},
    primitives::TxKind,
    state::AccountInfo,
    ExecuteEvm, MainBuilder, MainContext,
};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ReadRecord {
    Basic { address: Address },
    CodeByHash { code_hash: B256 },
    Storage { address: Address, slot: U256 },
    BlockHash { number: u64 },
}

struct TracingDb<DB> {
    inner: DB,
    reads: Vec<ReadRecord>,
}

impl<DB> TracingDb<DB> {
    fn new(inner: DB) -> Self {
        Self {
            inner,
            reads: Vec::new(),
        }
    }
}

impl<DB> revm::database::Database for TracingDb<DB>
where
    DB: revm::database::Database,
{
    type Error = DB::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.reads.push(ReadRecord::Basic { address });
        self.inner.basic(address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.reads.push(ReadRecord::CodeByHash { code_hash });
        self.inner.code_by_hash(code_hash)
    }

    fn storage(&mut self, address: Address, slot: U256) -> Result<U256, Self::Error> {
        self.reads.push(ReadRecord::Storage { address, slot });
        self.inner.storage(address, slot)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.reads.push(ReadRecord::BlockHash { number });
        self.inner.block_hash(number)
    }
}

fn minimal_erc20_transfer_bytecode() -> Bytecode {
    Bytecode::new_raw(
        vec![
            // sender_slot = keccak256(abi.encode(msg.sender, uint256(0)))
            0x33, // CALLER
            0x60, 0x00, // PUSH1 0x00
            0x52, // MSTORE
            0x60, 0x00, // PUSH1 0x00
            0x60, 0x20, // PUSH1 0x20
            0x52, // MSTORE
            0x60, 0x40, // PUSH1 0x40
            0x60, 0x00, // PUSH1 0x00
            0x20, // SHA3
            // balanceOf[msg.sender] = balanceOf[msg.sender] - amount
            0x80, // DUP1              stack: sender_slot, sender_slot
            0x54, // SLOAD             stack: sender_slot, sender_balance
            0x60, 0x24, // PUSH1 0x24
            0x35, // CALLDATALOAD      stack: sender_slot, sender_balance, amount
            0x90, // SWAP1             stack: sender_slot, amount, sender_balance
            0x03, // SUB               stack: sender_slot, sender_balance - amount
            0x90, // SWAP1             stack: new_balance, sender_slot
            0x55, // SSTORE
            // recipient_slot = keccak256(abi.encode(recipient, uint256(0)))
            0x60, 0x04, // PUSH1 0x04
            0x35, // CALLDATALOAD recipient
            0x60, 0x00, // PUSH1 0x00
            0x52, // MSTORE
            0x60, 0x00, // PUSH1 0x00
            0x60, 0x20, // PUSH1 0x20
            0x52, // MSTORE
            0x60, 0x40, // PUSH1 0x40
            0x60, 0x00, // PUSH1 0x00
            0x20, // SHA3
            // balanceOf[recipient] = balanceOf[recipient] + amount
            0x80, // DUP1
            0x54, // SLOAD
            0x60, 0x24, // PUSH1 0x24
            0x35, // CALLDATALOAD amount
            0x01, // ADD
            0x90, // SWAP1
            0x55, // SSTORE
            // return true
            0x60, 0x01, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
        ]
        .into(),
    )
}

fn balance_slot(owner: Address) -> U256 {
    let mut encoded = [0u8; 64];

    encoded[12..32].copy_from_slice(owner.as_slice());
    encoded[63] = 0;

    U256::from_be_bytes(keccak256(encoded).0)
}

fn transfer_calldata(recipient: Address, amount: U256) -> Bytes {
    let mut calldata = Vec::with_capacity(4 + 32 + 32);

    calldata.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);

    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(recipient.as_slice());

    let amount_bytes = amount.to_be_bytes::<32>();
    calldata.extend_from_slice(&amount_bytes);

    calldata.into()
}

fn main() {
    let mut db = CacheDB::new(EmptyDB::default());

    let sender = Address::from([0x11; 20]);
    let recipient = Address::from([0x33; 20]);
    let erc20 = Address::from([0x22; 20]);
    let bytecode = minimal_erc20_transfer_bytecode();

    let erc20_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 1,
        code_hash: bytecode.hash_slow(),
        code: Some(bytecode),
        account_id: None,
    };

    db.insert_account_info(erc20, erc20_info);

    let sender_info = AccountInfo {
        balance: U256::from(1_000_000_000_000_000_000u128),
        nonce: 0,
        code_hash: B256::ZERO,
        code: None,
        account_id: None,
    };

    db.insert_account_info(sender, sender_info);

    let sender_balance_slot = balance_slot(sender);
    let recipient_balance_slot = balance_slot(recipient);

    db.insert_account_storage(erc20, sender_balance_slot, U256::from(1_000u64))
        .unwrap();

    db.insert_account_storage(erc20, recipient_balance_slot, U256::ZERO)
        .unwrap();

    println!("sender:    {sender:?}");
    println!("recipient: {recipient:?}");
    println!("erc20:     {erc20:?}");

    println!("sender balance slot:    {sender_balance_slot}");
    println!("recipient balance slot: {recipient_balance_slot}");

    println!("db ready");

    let amount = U256::from(250u64);

    let tx = TxEnv {
        caller: sender,
        kind: TxKind::Call(erc20),
        data: transfer_calldata(recipient, amount),
        value: U256::ZERO,
        gas_limit: 1_000_000,
        ..Default::default()
    };

    let tracing_db = TracingDb::new(db);

    let mut evm = Context::mainnet().with_db(tracing_db).build_mainnet();

    let result = evm.transact(tx).unwrap();

    println!("execution result: {:#?}", result.result);

    assert!(result.result.is_success(), "transfer tx should succeed");

    let sender_account = result
        .state
        .get(&erc20)
        .expect("erc20 account should be touched");

    let sender_slot = sender_account
        .storage
        .get(&sender_balance_slot)
        .expect("sender balance slot should be written");

    let recipient_slot = sender_account
        .storage
        .get(&recipient_balance_slot)
        .expect("recipient balance slot should be written");

    assert_eq!(sender_slot.original_value(), U256::from(1_000u64));
    assert_eq!(sender_slot.present_value(), U256::from(750u64));

    assert_eq!(recipient_slot.original_value(), U256::ZERO);
    assert_eq!(recipient_slot.present_value(), U256::from(250u64));

    println!(
        "sender token balance:    {} -> {}",
        sender_slot.original_value(),
        sender_slot.present_value()
    );
    println!(
        "recipient token balance: {} -> {}",
        recipient_slot.original_value(),
        recipient_slot.present_value()
    );

    let reads = &evm.ctx.journaled_state.database.reads;

    println!("\nread-set:");
    for read in reads {
        println!("{read:?}");
    }

    let read_storage_slots: BTreeSet<(Address, U256)> = reads
        .iter()
        .filter_map(|read| match read {
            ReadRecord::Storage { address, slot } => Some((*address, *slot)),
            _ => None,
        })
        .collect();

    let expected_slots = BTreeSet::from([
        (erc20, sender_balance_slot),
        (erc20, recipient_balance_slot),
    ]);

    assert_eq!(
        read_storage_slots, expected_slots,
        "ERC20 transfer should read exactly sender and recipient balance slots"
    );

    let erc20_account = result
        .state
        .get(&erc20)
        .expect("erc20 account should be touched");

    let written_storage_slots: BTreeSet<(Address, U256)> = erc20_account
        .storage
        .keys()
        .map(|slot| (erc20, *slot))
        .collect();

    println!("\nwrite-set:");
    for (address, slot) in &written_storage_slots {
        let value = &result.state[address].storage[slot];
        println!(
            "address={address:?} slot={slot} original={} present={}",
            value.original_value(),
            value.present_value()
        );
    }

    assert_eq!(
        written_storage_slots, expected_slots,
        "ERC20 transfer should write exactly sender and recipient balance slots"
    );
}
