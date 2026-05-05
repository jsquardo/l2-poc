use alloy_primitives::{Address, Bytes, B256, U256};
use revm::{
    bytecode::Bytecode,
    context::{Context, TxEnv},
    database::{Database, InMemoryDB},
    primitives::TxKind,
    state::AccountInfo,
    ExecuteEvm, MainBuilder, MainContext,
};

#[allow(dead_code)]
#[derive(Debug)]
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

impl<DB> Database for TracingDb<DB>
where
    DB: Database,
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

fn main() {
    let mut db = InMemoryDB::default();

    let caller = Address::from([0x11; 20]);
    let contract = Address::from([0x22; 20]);

    let caller_info = AccountInfo {
        balance: U256::from(1_000_000_000_000_000_000u128),
        nonce: 0,
        code_hash: Default::default(),
        code: None,
        account_id: None,
    };

    let bytecode = Bytecode::new_raw(
        vec![
            0x60, 0x42, // PUSH1 0x42
            0x60, 0x00, // PUSH1 0x00
            0x55, // SSTORE
            0x00, // STOP
        ]
        .into(),
    );

    let contract_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 1,
        code_hash: bytecode.hash_slow(),
        code: Some(bytecode),
        account_id: None,
    };

    db.insert_account_info(caller, caller_info);
    db.insert_account_info(contract, contract_info);

    let tracing_db = TracingDb::new(db);

    let tx = TxEnv {
        caller,
        kind: TxKind::Call(contract),
        data: Bytes::new(),
        value: U256::ZERO,
        gas_limit: 1_000_000,
        ..Default::default()
    };

    let mut evm = Context::mainnet().with_db(tracing_db).build_mainnet();

    let result = evm.transact(tx).unwrap();

    println!("execution result: {:#?}", result.result);

    println!("\nwrite-set:");
    for (address, account) in result.state {
        for (slot, value) in account.storage {
            println!(
                "address={address:?} slot={slot} original={} present={}",
                value.original_value(),
                value.present_value()
            );
        }
    }

    println!("\nread-set:");
    for read in evm.ctx.journaled_state.database.reads {
        println!("{read:?}");
    }
}
