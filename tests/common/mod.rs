#![allow(dead_code)]
#![allow(unused_macros)]

pub mod program_setup;
pub mod log;

use solana_program::{
    pubkey::Pubkey,
    instruction::{Instruction, AccountMeta}, system_instruction, native_token::LAMPORTS_PER_SOL, account_info::{Account, AccountInfo},
};
use solana_program_test::ProgramTestContext;
use solana_sdk::{signature::{Keypair}, transaction::Transaction, signer::Signer};
use assert_matches::assert_matches;
use std::{str::FromStr, collections::HashMap};
use elusiv::{types::{U256, RawU256}, instruction::{UserAccount, WritableUserAccount}, fields::fr_to_u256_le_repr, state::program_account::MultiAccountProgramAccount, proof::precompute::PrecomputesAccount};
use elusiv::fields::fr_to_u256_le;
use elusiv::processor::BaseCommitmentHashRequest;
use elusiv::state::{StorageAccount, NullifierAccount, program_account::{PDAAccount, MultiAccountAccount, MultiAccountAccountData}};

const DEFAULT_START_BALANCE: u64 = LAMPORTS_PER_SOL;

pub struct Actor {
    pub keypair: Keypair,
    pub pubkey: Pubkey,

    // Due to the InvalidRentPayingAccount error, we need to give our client a starting balance (= zero)
    pub start_balance: u64,
}

impl Clone for Actor {
    fn clone(&self) -> Self {
        let keypair = Keypair::from_bytes(&self.keypair.to_bytes()).unwrap();
        Actor { keypair, pubkey: self.pubkey, start_balance: self.start_balance }
    }
}

impl Actor {
    pub async fn new(
        context: &mut ProgramTestContext,
    ) -> Self {
        let keypair = Keypair::new();
        let pubkey = keypair.pubkey();

        airdrop(&pubkey, DEFAULT_START_BALANCE, context).await;

        Actor {
            keypair,
            pubkey,
            start_balance: DEFAULT_START_BALANCE,
        }
    }

    /// Returns the account's balance - start_balance - failed_signatures * lamports_per_signature
    pub async fn balance(&self, context: &mut ProgramTestContext) -> u64 {
        get_balance(&self.pubkey, context).await - self.start_balance
    }

    pub async fn airdrop(&self, lamports: u64, context: &mut ProgramTestContext) {
        airdrop(&self.pubkey, lamports, context).await
    }
}

pub async fn get_balance(pubkey: &Pubkey, context: &mut ProgramTestContext) -> u64 {
    context.banks_client.get_account(*pubkey).await.unwrap().unwrap().lamports
}

pub async fn account_does_exist(pubkey: Pubkey, context: &mut ProgramTestContext) -> bool {
    matches!(context.banks_client.get_account(pubkey).await.unwrap(), Some(_))
}

pub async fn account_does_not_exist(pubkey: Pubkey, context: &mut ProgramTestContext) -> bool {
    !account_does_exist(pubkey, context).await
}

pub async fn get_data(context: &mut ProgramTestContext, id: Pubkey) -> Vec<u8> {
    context.banks_client.get_account(id).await.unwrap().unwrap().data
}

pub async fn get_account_cost(context: &mut ProgramTestContext, size: usize) -> u64 {
    let rent = context.banks_client.get_rent().await.unwrap();
    rent.minimum_balance(size)
}

pub async fn airdrop(account: &Pubkey, lamports: u64, context: &mut ProgramTestContext) {
    let mut tx = Transaction::new_with_payer(
        &[
            nonce_instruction(
                system_instruction::transfer(&context.payer.pubkey(), account, lamports)
            )
        ],
        Some(&context.payer.pubkey())
    );
    tx.sign(&[&context.payer], context.last_blockhash);
    assert_matches!(context.banks_client.process_transaction(tx).await, Ok(()));
}

#[allow(deprecated)]
pub async fn lamports_per_signature(context: &mut ProgramTestContext) -> u64 {
    context.banks_client.get_fees().await.unwrap().0.lamports_per_signature
}

// Account getters
macro_rules! queue_mut {
    ($id: ident, $ty: ty, $ty_account: ty, $data: expr) => {
        let mut queue = <$ty_account>::new($data).unwrap();
        let mut $id = <$ty>::new(&mut queue);
    };
}

macro_rules! queue {
    ($id: ident, $ty: ty, $ty_account: ty, $offset: expr, $context: expr) => {
        let mut queue = get_data($context, <$ty_account>::find($offset).0).await;
        let mut queue = <$ty_account>::new(&mut queue[..]).unwrap();
        let $id = <$ty>::new(&mut queue);
    };
}

macro_rules! sized_account {
    ($id: ident, $ty: ty, $offset: expr, $data: ident) => {
        let $id = <$ty>::new(&mut $data).unwrap();
    };
}

/// mut? $id: ident, $ty: ty, $offset: expr, $context: ident
macro_rules! pda_account {
    ($id: ident, $ty: ty, $offset: expr, $context: expr) => {
        pda_account!(data data, $ty, $offset, $context);
        let $id = <$ty>::new(&mut data).unwrap();
    };
    (mut $id: ident, $ty: ty, $offset: expr, $context: expr) => {
        pda_account!(data data, $ty, $offset, $context);
        let mut $id = <$ty>::new(&mut data).unwrap();
    };

    (data $data: ident, $ty: ty, $offset: expr, $context: expr) => {
        let pk = <$ty>::find($offset).0;
        let mut $data = &mut get_data($context, pk).await[..];
    };
}

macro_rules! account_info {
    ($id: ident, $pk: expr, $context: expr) => {
        let mut a = $context.banks_client.get_account($pk).await.unwrap().unwrap();
        let (mut lamports, mut d, owner, executable, epoch) = a.get();

        let $id = solana_program::account_info::AccountInfo::new(
            &$pk,
            false,
            false,
            &mut lamports,
            &mut d,
            &owner,
            executable,
            epoch
        );
    };
}


macro_rules! multi_account {
    ($id: ident, $ty: ty) => {
        pub async fn $id<F>(
            context: &mut ProgramTestContext,
            pda_offset: Option<u32>,
            f: F,
        ) where
            F: Fn(&$ty),
        {
            let mut data = get_data(context, <$ty>::find(pda_offset).0).await;
            let pks = MultiAccountAccountData::<{<$ty>::COUNT}>::new(&data).unwrap();
            let keys = pks.pubkeys.iter().map(|p| p.option().unwrap()).collect::<Vec<Pubkey>>();
        
            let mut v = vec![];
            for &key in keys.iter() {
                let a = context.banks_client.get_account(key).await.unwrap().unwrap();
                v.push(a);
            }
        
            let accs = v.iter_mut();
            let mut sub_accounts = HashMap::new();
            for (i, a) in accs.enumerate() {
                let (lamports, d, owner, executable, epoch) = a.get();
                let sub_account = AccountInfo::new(&keys[i], false, false, lamports, d, owner, executable, epoch);
                sub_accounts.insert(i, sub_account);
            }
        
            let map: HashMap<usize, &AccountInfo> = sub_accounts.iter().map(|(&i, x)| (i, x)).collect();
        
            let account = <$ty>::new(&mut data, map).unwrap();
            f(&account)
        }
    };
}

multi_account!(storage_account, StorageAccount);
multi_account!(nullifier_account, NullifierAccount);
multi_account!(precomputes_account, PrecomputesAccount);

#[allow(unused_imports)] pub(crate) use queue;
#[allow(unused_imports)] pub(crate) use queue_mut;
#[allow(unused_imports)] pub(crate) use pda_account;
#[allow(unused_imports)] pub(crate) use sized_account;
#[allow(unused_imports)] pub(crate) use account_info;

const STORAGE_SUB_ACCOUNT_SIZE: usize = StorageAccount::COUNT;

pub async fn storage_accounts(context: &mut ProgramTestContext) ->
(
    Vec<Pubkey>,
    [UserAccount; STORAGE_SUB_ACCOUNT_SIZE],
    [WritableUserAccount; STORAGE_SUB_ACCOUNT_SIZE],
)
{
    let data = get_data(context, StorageAccount::find(None).0).await;
    let pubkeys: Vec<Pubkey> = MultiAccountAccountData::<{StorageAccount::COUNT}>::new(&data).unwrap()
        .pubkeys.iter().map(|x| x.option().unwrap()).collect();
    let (read_only, writeable) = multi_account_pubkeys(&pubkeys);

    (pubkeys, read_only.try_into().unwrap(), writeable.try_into().unwrap())
}

const NULLIFIER_SUB_ACCOUNT_SIZE: usize = NullifierAccount::COUNT;

pub async fn nullifier_accounts(mt_index: u32, context: &mut ProgramTestContext) ->
(
    Vec<Pubkey>,
    [UserAccount; NULLIFIER_SUB_ACCOUNT_SIZE],
    [WritableUserAccount; NULLIFIER_SUB_ACCOUNT_SIZE],
)
{
    let data = get_data(context, NullifierAccount::find(Some(mt_index)).0).await;
    let pubkeys: Vec<Pubkey> = MultiAccountAccountData::<{NullifierAccount::COUNT}>::new(&data).unwrap()
        .pubkeys.iter().map(|x| x.option().unwrap()).collect();
    let (read_only, writeable) = multi_account_pubkeys(&pubkeys);

    (pubkeys, read_only.try_into().unwrap(), writeable.try_into().unwrap())
}

fn multi_account_pubkeys(pubkeys: &[Pubkey]) -> (Vec<UserAccount>, Vec<WritableUserAccount>) {
    (
        pubkeys.iter().map(|p| UserAccount(*p)).collect(),
        pubkeys.iter().map(|p| WritableUserAccount(*p)).collect(),
    )
}

use self::program_setup::set_account;

/// Adds random nonce bytes at the end of the ix data
/// - prevents rejection of previously failed ix times without repeated execution
pub fn nonce_instruction(ix: Instruction) -> Instruction {
    let mut ix = ix;
    for _ in 0..8 {
        ix.data.push(rand::random());
    }
    ix
}

/// Replaces all accounts through invalid accounts with valid data and lamports
/// - returns the fuzzed instructions and accorsing signers
pub async fn invalid_accounts_fuzzing(
    ix: &Instruction,
    context: &mut ProgramTestContext,
    original_signer: &Actor,
) -> Vec<(Instruction, Actor)> {
    let mut result = Vec::new();
    for (i, acc) in ix.accounts.iter().enumerate() {
        let signer = if !acc.is_signer { (*original_signer).clone() } else { Actor::new(context).await };
        let mut ix = ix.clone();

        // Clone data and lamports
        let id = acc.pubkey;
        let accounts_exists = account_does_exist(id, context).await;
        let data = if accounts_exists { get_data(context, id).await } else { vec![] };
        let lamports = if accounts_exists { get_balance(&id, context).await } else { 100_000 };
        let new_pubkey = Pubkey::new_unique();
        set_account(context, &new_pubkey, data, lamports).await;

        if acc.is_writable {
            ix.accounts[i] = AccountMeta::new(new_pubkey, false);
        } else {
            ix.accounts[i] = AccountMeta::new_readonly(new_pubkey, false);
        }

        result.push((ix, signer));
    }
    result
}

/// All fuzzed ix variants should fail and the original ix should afterwards succeed
/// - prefix_ixs are used to e.g. supply compute budget requests without fuzzing those ixs
pub async fn test_instruction_fuzzing(
    prefix_ixs: &[Instruction],
    valid_ix: Instruction,
    signer: &mut Actor,
    context: &mut ProgramTestContext
) {
    let invalid_instructions = invalid_accounts_fuzzing(
        &valid_ix,
        context,
        signer,
    ).await;

    for (ix, signer) in invalid_instructions {
        let mut ixs = prefix_ixs.to_vec();
        ixs.push(ix);

        let mut signer = signer.clone();
        tx_should_fail(&ixs, &mut signer, context).await;
    }

    let mut ixs = prefix_ixs.to_vec();
    ixs.push(valid_ix);
    tx_should_succeed(&ixs, signer, context).await;
}

async fn generate_and_sign_tx(
    ixs: &[Instruction],
    signer: &mut Actor,
    context: &mut ProgramTestContext,
) -> Transaction {
    let ixs: Vec<Instruction> = ixs.iter()
        .map(|ix| nonce_instruction(ix.clone()))
        .collect();
    let mut tx = Transaction::new_with_payer(
        &ixs,
        Some(&signer.pubkey)
    );
    tx.sign(
        &[&signer.keypair],
        context.banks_client.get_latest_blockhash().await.unwrap()
    );

    tx
}

// Succesful transactions
pub async fn tx_should_succeed(
    ixs: &[Instruction],
    signer: &mut Actor,
    context: &mut ProgramTestContext,
) {
    let tx = generate_and_sign_tx(ixs, signer, context).await;
    assert_matches!(context.banks_client.process_transaction(tx).await, Ok(()));
}

pub async fn ix_should_succeed(
    ix: Instruction,
    signer: &mut Actor,
    context: &mut ProgramTestContext,
) {
    tx_should_succeed(&[ix], signer, context).await
}

// Failing transactions
pub async fn tx_should_fail(
    ixs: &[Instruction],
    signer: &mut Actor,
    context: &mut ProgramTestContext,
) {
    let tx = generate_and_sign_tx(ixs, signer, context).await;
    assert_matches!(context.banks_client.process_transaction(tx).await, Err(_));

    // To compensate for failure, we airdrop
    airdrop(&signer.pubkey, lamports_per_signature(context).await, context).await;
}

pub async fn ix_should_fail(
    ix: Instruction,
    signer: &mut Actor,
    context: &mut ProgramTestContext,
) {
    tx_should_fail(&[ix], signer, context).await
}

pub fn u256_from_str(str: &str) -> U256 {
    fr_to_u256_le(&ark_bn254::Fr::from_str(str).unwrap())
}

pub fn u256_from_str_skip_mr(str: &str) -> U256 {
    fr_to_u256_le_repr(&ark_bn254::Fr::from_str(str).unwrap())
}

pub fn base_commitment_request(
    base_commitment: &str,
    commitment: &str,
    amount: u64,
    token_id: u16,
    fee_version: u32,
    min_batching_rate: u32,
) -> BaseCommitmentHashRequest {
    BaseCommitmentHashRequest {
        base_commitment: RawU256::new(u256_from_str_skip_mr(base_commitment)),
        commitment: RawU256::new(u256_from_str_skip_mr(commitment)),
        amount,
        token_id,
        fee_version,
        min_batching_rate,
    }
}