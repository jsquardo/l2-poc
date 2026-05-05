use alloy_primitives::{Address, Bytes, U256};
use revm::{
    bytecode::Bytecode,
    context::{Context, TxEnv},
    database::InMemoryDB,
    primitives::TxKind,
    state::AccountInfo,
    ExecuteEvm, MainBuilder, MainContext,
};

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

    let tx = TxEnv {
        caller,
        kind: TxKind::Call(contract),
        data: Bytes::new(),
        value: U256::ZERO,
        gas_limit: 1_000_000,
        ..Default::default()
    };

    let mut evm = Context::mainnet().with_db(db).build_mainnet();

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
}
