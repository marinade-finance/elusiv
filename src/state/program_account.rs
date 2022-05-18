use solana_program::account_info::AccountInfo;
use crate::bytes::SerDe;
use crate::error::ElusivError;
use crate::types::U256;
use crate::macros::{ pda, account_data_mut, account_data };

/// This trait is used by the elusiv_instruction macro
pub trait PDAAccount {
    const SEED: &'static [u8];

    /// Returns the PDA seed for generating a PDA
    fn pda_seed(offsets: &[u64]) -> &[&[u8]] {
        let mut seed: Vec<&[u8]> = vec![Self::SEED];
        seed.extend(offsets.iter().map(|o| &o.to_le_bytes()[..]));
        &seed
    }
} 

const MAX_ACCOUNT_SIZE: usize = 10_000_000;

/// Allows for storing data in an array that cannot be stored in a single Solana account
/// - this is achieved by having SIZE AccountInfos in get_array_accounts
/// - BigArrayAccount takes care of parsing the data stored in those accounts
/// - these array_accounts are PDA accounts generated by extending the BigArrayAccount's pda_seed
pub trait BigArrayAccount<'a>: PDAAccount {
    type T: SerDe<T=Self::T>;

    const SIZE: usize;
    fn get_array_accounts(&self) -> Vec<AccountInfo<'a>>;

    const MAX_VALUES_PER_ACCOUNT: usize = MAX_ACCOUNT_SIZE / Self::T::SIZE;
    const ACCOUNTS_COUNT: usize = Self::SIZE / Self::MAX_VALUES_PER_ACCOUNT + (if Self::SIZE % Self::MAX_VALUES_PER_ACCOUNT == 0 { 0 } else { 1 });

    // indices in this implementation are always the external array indices and not byte-indices!
    fn account_and_local_index(&self, index: usize) -> (usize, usize) {
        let account_index = index / Self::MAX_VALUES_PER_ACCOUNT;
        (account_index, index - account_index * Self::MAX_VALUES_PER_ACCOUNT)
    }

    fn account_data(&self, index: usize) -> &'a mut [u8] {
        let (account_index, local_index) = self.account_and_local_index(index);
        let account = self.get_array_accounts()[account_index];
        &mut account_data_mut!(account)[local_index * Self::T::SIZE..(local_index + 1) * Self::T::SIZE]
    }

    fn get(&self, index: usize) -> Self::T {
        Self::T::deserialize(self.account_data(index))
    }

    fn set(&self, index: usize, value: Self::T) {
        Self::T::write(value, self.account_data(index));
    }

    fn get_mut_array_slice(&self, start_index: usize, end_index: usize) -> &mut [u8] {
        let (start_account_index, start_local_index) = self.account_and_local_index(start_index);
        let (end_account_index, end_local_index) = self.account_and_local_index(end_index);

        let data = Vec::new();
        for i in start_account_index..=end_account_index {
            data.extend(account_data!(self.get_array_accounts()[i]));
        }

        &mut data[start_local_index * Self::T::SIZE..(end_local_index + (end_account_index * Self::MAX_VALUES_PER_ACCOUNT)) * Self::T::SIZE]
    }

    fn get_full_array(&self) -> &[u8] {
        let s = self.get_mut_array_slice(0, Self::SIZE - 1);
        &s
    }

    fn array_accounts_pdas(offsets: &[u64]) -> Vec<solana_program::pubkey::Pubkey> {
        (0..Self::ACCOUNTS_COUNT).collect::<Vec::<usize>>().iter().map(|&i| {
            let offsets = offsets.to_vec();
            offsets.push(i as u64);
            pda!(Self::pda_seed(&offsets))
        })
        .collect()
    }
}

/// Account used for computations that require multiple transactions to finish
/// - `is_active`: if false: the account can be reset and a new computation can start, if true: clients can participate in the current computation by sending tx
/// - `round`: the index of the last round
/// - `total_rounds`: the count of all rounds
/// - `fee_payer`: account that payed the fees for the whole computation up-front (will be reimbursed after a successfull computation)
pub trait PartialComputationAccount {
    fn get_is_active(&self) -> bool;
    fn set_is_active(&mut self, value: bool);

    fn get_round(&self) -> u64;
    fn set_round(&mut self, value: u64);

    fn get_total_rounds(&self) -> u64;
    fn set_total_rounds(&mut self, value: u64);

    fn get_fee_payer(&self) -> U256;
    fn set_fee_payer(&mut self, value: U256);
}