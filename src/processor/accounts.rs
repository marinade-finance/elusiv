use std::collections::HashSet;
use borsh::{BorshSerialize, BorshDeserialize};
use solana_program::{
    entrypoint::ProgramResult,
    account_info::AccountInfo,
    sysvar::Sysvar,
    rent::Rent,
};
use crate::state::{
    governor::{GovernorAccount, PoolAccount, FeeCollectorAccount, DEFAULT_COMMITMENT_BATCHING_RATE},
    program_account::{PDAAccount, SizedAccount, MultiAccountAccount, ProgramAccount, HeterogenMultiAccountAccount},
    StorageAccount,
    queue::{CommitmentQueueAccount, BaseCommitmentQueueAccount},
    fee::FeeAccount, NullifierAccount,
};
use crate::commitment::{CommitmentHashingAccount};
use crate::error::ElusivError::{InvalidInstructionData, InvalidFeeVersion};
use crate::macros::*;
use crate::bytes::{BorshSerDeSized, is_zero};
use crate::types::U256;
use super::utils::*;

#[derive(BorshSerialize, BorshDeserialize, BorshSerDeSized)]
pub enum SingleInstancePDAAccountKind {
    CommitmentHashingAccount,
    CommitmentQueueAccount,
    PoolAccount,
    FeeCollectorAccount,

    StorageAccount,
    NullifierAccount,
}

/// Opens one single instance `PDAAccount`, as long this PDA does not already exist
pub fn open_single_instance_account<'a>(
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,

    kind: SingleInstancePDAAccountKind,
) -> ProgramResult {
    match kind {
        SingleInstancePDAAccountKind::CommitmentHashingAccount => {
            open_pda_account_without_offset::<CommitmentHashingAccount>(payer, pda_account)
        }
        SingleInstancePDAAccountKind::CommitmentQueueAccount => {
            open_pda_account_without_offset::<CommitmentQueueAccount>(payer, pda_account)
        }
        SingleInstancePDAAccountKind::PoolAccount => {
            open_pda_account_without_offset::<PoolAccount>(payer, pda_account)
        }
        SingleInstancePDAAccountKind::FeeCollectorAccount => {
            open_pda_account_without_offset::<FeeCollectorAccount>(payer, pda_account)
        }
        SingleInstancePDAAccountKind::StorageAccount => {
            open_pda_account_without_offset::<StorageAccount>(payer, pda_account)
        }
        SingleInstancePDAAccountKind::NullifierAccount => {
            open_pda_account_without_offset::<NullifierAccount>(payer, pda_account)
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, BorshSerDeSized)]
pub enum MultiInstancePDAAccountKind {
    BaseCommitmentQueueAccount
}

/// Opens one multi instance `PDAAccount` with the offset `Some(pda_offset)`, as long this PDA does not already exist
pub fn open_multi_instance_account<'a>(
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,

    kind: MultiInstancePDAAccountKind,
    pda_offset: u64,
) -> ProgramResult {
    match kind {
        MultiInstancePDAAccountKind::BaseCommitmentQueueAccount => {
            open_pda_account_with_offset::<BaseCommitmentQueueAccount>(payer, pda_account, pda_offset)
        }
    }
}

pub fn open_pda_account_with_offset<'a, T: PDAAccount + SizedAccount>(
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,
    pda_offset: u64,
) -> ProgramResult {
    let account_size = T::SIZE;
    let (pk, bump) = T::find(Some(pda_offset));
    let seed = vec![
        T::SEED.to_vec(),
        u64::to_le_bytes(pda_offset).to_vec(),
        vec![bump]
    ];
    let signers_seeds: Vec<&[u8]> = seed.iter().map(|x| &x[..]).collect();
    guard!(pk == *pda_account.key, InvalidInstructionData);

    create_pda_account(payer, pda_account, account_size, bump, &signers_seeds)
}

pub fn open_pda_account_without_offset<'a, T: PDAAccount + SizedAccount>(
    payer: &AccountInfo<'a>,
    pda_account: &AccountInfo<'a>,
) -> ProgramResult {
    let account_size = T::SIZE;
    let (pk, bump) = T::find(None);
    let seed = vec![
        T::SEED.to_vec(),
        vec![bump]
    ];
    let signers_seeds: Vec<&[u8]> = seed.iter().map(|x| &x[..]).collect();
    guard!(pk == *pda_account.key, InvalidInstructionData);

    create_pda_account(payer, pda_account, account_size, bump, &signers_seeds)
}

/// Setup the StorageAccount with it's 7 subaccounts
pub fn setup_storage_account(
    storage_account: &mut StorageAccount,
) -> ProgramResult {
    // Note: we don't zero-check these accounts, since we will never access data that has not been set by the program
    verify_heterogen_sub_accounts(storage_account, false)?;
    setup_multi_account_account(storage_account)
}

/// Setup the `GovernorAccount` with the default values
/// - Note: there is no way of upgrading it atm
pub fn setup_governor_account<'a>(
    payer: &AccountInfo<'a>,
    governor_account: &AccountInfo<'a>,
) -> ProgramResult {
    open_pda_account_without_offset::<GovernorAccount>(payer, governor_account)?;

    let mut data = &mut governor_account.data.borrow_mut()[..];
    let mut governor = GovernorAccount::new(&mut data)?;

    governor.set_commitment_batching_rate(&DEFAULT_COMMITMENT_BATCHING_RATE);

    Ok(())
}

/// Setup a new `FeeAccount`
/// - Note: there is no way of upgrading the program fees atm
pub fn init_new_fee_version<'a>(
    payer: &AccountInfo<'a>,
    governor: &GovernorAccount,
    new_fee: &AccountInfo<'a>,

    fee_version: u64,

    lamports_per_tx: u64,
    base_commitment_fee: u64,
    proof_fee: u64,
    relayer_hash_tx_fee: u64,
    relayer_proof_tx_fee: u64,
    relayer_proof_reward: u64,
) -> ProgramResult {
    guard!(fee_version == governor.get_fee_version(), InvalidFeeVersion);
    open_pda_account_with_offset::<FeeAccount>(payer, new_fee, fee_version)?;

    let mut data = new_fee.data.borrow_mut();
    let mut fee = FeeAccount::new(&mut data[..])?;

    fee.setup(lamports_per_tx, base_commitment_fee, proof_fee, relayer_hash_tx_fee, relayer_proof_tx_fee, relayer_proof_reward)
}

fn setup_multi_account_account<'a, T: MultiAccountAccount<'a>>(
    account: &mut T,
) -> ProgramResult {
    guard!(!account.pda_initialized(), InvalidInstructionData);

    // Set all pubkeys
    let mut pks = Vec::new();
    for i in 0..T::COUNT {
        pks.push(account.get_account(i).key.to_bytes());
    }
    account.set_all_pubkeys(&pks);

    // Check for account duplicates
    let set: HashSet<U256> = account.get_all_pubkeys().clone().drain(..).collect();
    guard!(set.len() == StorageAccount::COUNT, InvalidInstructionData);

    account.set_pda_initialized(true);
    guard!(account.pda_initialized(), InvalidInstructionData);

    Ok(())
}

/// Verifies that an account with `data_len` > 10 KiB (non PDA) is formatted correctly
fn verify_extern_data_account(
    account: &AccountInfo,
    data_len: usize,
    check_zeroness: bool,
) -> ProgramResult {
    guard!(account.data_len() == data_len, InvalidInstructionData);
    if check_zeroness {
        guard!(is_zero(&account.data.borrow()[..]), InvalidInstructionData);
    }

    // Check rent-exemption
    if cfg!(test) { // only unit-testing (since we have no ledger there)
        guard!(account.lamports() >= u64::MAX / 2, InvalidInstructionData);
    } else {
        guard!(account.lamports() >= Rent::get()?.minimum_balance(data_len), InvalidInstructionData);
    }

    // Check ownership
    guard!(*account.owner == crate::id(), InvalidInstructionData);

    Ok(())
}

// Verifies the user-supplied sub-accounts
fn verify_heterogen_sub_accounts<'a, T: HeterogenMultiAccountAccount<'a>>(
    storage_account: &T,
    check_zeroness: bool,
) -> ProgramResult {
    for i in 0..T::COUNT {
        verify_extern_data_account(
            storage_account.get_account(i),
            if i < T::COUNT - 1 {
                T::INTERMEDIARY_ACCOUNT_SIZE
            } else {
                T::LAST_ACCOUNT_SIZE
            },
            check_zeroness
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::program_account::MultiAccountProgramAccount;

    #[test]
    fn test_storage_account_valid() {
        let mut data = vec![0; StorageAccount::SIZE];
        generate_storage_accounts_valid_size!(accounts);
        let storage_account = StorageAccount::new(&mut data, accounts).unwrap();
        verify_heterogen_sub_accounts(&storage_account, false).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_storage_account_invalid_size() {
        let mut data = vec![0; StorageAccount::SIZE];

        generate_storage_accounts!(accounts, [
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::INTERMEDIARY_ACCOUNT_SIZE,
            StorageAccount::LAST_ACCOUNT_SIZE - 1,
        ]);

        let storage_account = StorageAccount::new(&mut data, accounts).unwrap();
        verify_heterogen_sub_accounts(&storage_account, false).unwrap();
    }
}