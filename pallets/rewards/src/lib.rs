// This file is part of the SORA network and Polkaswap app.

// Copyright (c) 2020, 2021, Polka Biome Ltd. All rights reserved.
// SPDX-License-Identifier: BSD-4-Clause

// Redistribution and use in source and binary forms, with or without modification,
// are permitted provided that the following conditions are met:

// Redistributions of source code must retain the above copyright notice, this list
// of conditions and the following disclaimer.
// Redistributions in binary form must reproduce the above copyright notice, this
// list of conditions and the following disclaimer in the documentation and/or other
// materials provided with the distribution.
//
// All advertising materials mentioning features or use of this software must display
// the following acknowledgement: This product includes software developed by Polka Biome
// Ltd., SORA, and Polkaswap.
//
// Neither the name of the Polka Biome Ltd. nor the names of its contributors may be used
// to endorse or promote products derived from this software without specific prior written permission.

// THIS SOFTWARE IS PROVIDED BY Polka Biome Ltd. AS IS AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL Polka Biome Ltd. BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
// BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS;
// OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
// STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! This pallet enables users to claim their rewards.
//!
//! There are following kinds of rewards:
//! * VAL for XOR owners
//! * PSWAP farming
//! * PSWAP NFT waifus

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::codec::{Decode, Encode};
use frame_support::dispatch::{DispatchErrorWithPostInfo, Weight};
use frame_support::storage::StorageMap as StorageMapTrait;
use frame_support::RuntimeDebug;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::H160;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_runtime::{Perbill, Percent};
use sp_std::prelude::*;

use assets::AssetIdOf;
use common::prelude::FixedWrapper;
#[cfg(feature = "include-real-files")]
use common::vec_push;
use common::{balance, eth, AccountIdOf, Balance, OnValBurned};

#[cfg(feature = "include-real-files")]
use hex_literal::hex;

pub use self::pallet::*;

pub mod migrations;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

type EthereumAddress = H160;
type WeightInfoOf<T> = <T as Config>::WeightInfo;

#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct RewardInfo {
    claimable: Balance,
    total: Balance,
}

impl RewardInfo {
    pub fn new(claimable: Balance, total: Balance) -> Self {
        Self { claimable, total }
    }
}

impl From<(Balance, Balance)> for RewardInfo {
    fn from(value: (Balance, Balance)) -> Self {
        RewardInfo::new(value.0, value.1)
    }
}

impl From<Balance> for RewardInfo {
    fn from(value: Balance) -> Self {
        RewardInfo::new(value, 0)
    }
}

pub const TECH_ACCOUNT_PREFIX: &[u8] = b"rewards";
pub const TECH_ACCOUNT_MAIN: &[u8] = b"main";

pub trait WeightInfo {
    fn claim() -> Weight;
    fn finalize_storage_migration(n: u32) -> Weight;
}

impl<T: Config> Pallet<T> {
    pub fn claimables(eth_address: &EthereumAddress) -> Vec<Balance> {
        vec![
            ValOwners::<T>::get(eth_address).claimable,
            PswapFarmOwners::<T>::get(eth_address),
            PswapWaifuOwners::<T>::get(eth_address),
        ]
    }

    fn current_vesting_ratio(elapsed: T::BlockNumber) -> Perbill {
        let max_percentage = T::MAX_VESTING_RATIO.deconstruct() as u32;
        if elapsed >= T::TIME_TO_SATURATION {
            Perbill::from_percent(max_percentage)
        } else {
            let elapsed_u32: u32 = elapsed.unique_saturated_into();
            let time_to_saturation: u32 = T::TIME_TO_SATURATION.unique_saturated_into();
            Perbill::from_rational_approximation(
                max_percentage * elapsed_u32,
                100_u32 * time_to_saturation,
            )
        }
    }

    fn claim_reward<M: StorageMapTrait<EthereumAddress, Balance>>(
        eth_address: &EthereumAddress,
        account_id: &AccountIdOf<T>,
        asset_id: &AssetIdOf<T>,
        reserves_acc: &T::TechAccountId,
        claimed: &mut bool,
        is_eligible: &mut bool,
    ) -> Result<(), DispatchErrorWithPostInfo> {
        if let Ok(balance) = M::try_get(eth_address) {
            *is_eligible = true;
            if balance > 0 {
                technical::Pallet::<T>::transfer_out(asset_id, reserves_acc, account_id, balance)?;
                M::insert(eth_address, 0);
                *claimed = true;
            }
        }
        Ok(())
    }

    fn claim_val_reward(
        eth_address: &EthereumAddress,
        account_id: &AccountIdOf<T>,
        asset_id: &AssetIdOf<T>,
        reserves_acc: &T::TechAccountId,
        claimed: &mut bool,
        is_eligible: &mut bool,
    ) -> Result<(), DispatchErrorWithPostInfo> {
        if let Ok(RewardInfo {
            claimable: amount,
            total,
        }) = ValOwners::<T>::try_get(eth_address)
        {
            *is_eligible = true;
            if amount > 0 {
                technical::Pallet::<T>::transfer_out(asset_id, reserves_acc, account_id, amount)?;
                ValOwners::<T>::mutate(eth_address, |v| {
                    *v = RewardInfo::new(0, total.saturating_sub(amount))
                });
                TotalValRewards::<T>::mutate(|v| *v = v.saturating_sub(amount));
                TotalClaimableVal::<T>::mutate(|v| *v = v.saturating_sub(amount));
                *claimed = true;
            }
        }
        Ok(())
    }
}

impl<T: Config> OnValBurned for Pallet<T> {
    fn on_val_burned(amount: Balance) {
        ValBurnedSinceLastVesting::<T>::mutate(|v| {
            *v = v.saturating_add(amount.saturating_sub(amount / 100))
        });
    }
}

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_support::traits::PalletVersion;
    use frame_support::transactional;
    use frame_system::pallet_prelude::*;
    use secp256k1::util::SIGNATURE_SIZE;
    use secp256k1::{RecoveryId, Signature};
    use sp_std::vec::Vec;

    use common::{PSWAP, VAL};

    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + assets::Config + technical::Config {
        /// How often the rewards data are being updated
        const UPDATE_FREQUENCY: BlockNumberFor<Self>;
        /// Vested amount is updated every `BLOCKS_PER_DAY` blocks
        const BLOCKS_PER_DAY: BlockNumberFor<Self>;
        /// Max number of addresses to be processed in one take
        const MAX_CHUNK_SIZE: usize;
        /// Max percentage of daily burned VAL that can be vested as rewards
        const MAX_VESTING_RATIO: Percent;
        /// The amount of time until vesting ratio reaches saturation at `MAX_VESTING_RATIO`
        const TIME_TO_SATURATION: BlockNumberFor<Self>;
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(now: T::BlockNumber) -> Weight {
            let mut consumed_weight: Weight = 0;

            if (now % T::BLOCKS_PER_DAY).is_zero() {
                if TotalValRewards::<T>::get() == TotalClaimableVal::<T>::get() {
                    // All VAL has been vested
                    CurrentClaimableVal::<T>::put(0);
                    return T::DbWeight::get().reads_writes(2, 2);
                }

                let val_burned = ValBurnedSinceLastVesting::<T>::get();
                let vesting_ratio = Self::current_vesting_ratio(now);
                let vested_amount = vesting_ratio * val_burned;
                CurrentClaimableVal::<T>::put(vested_amount);
                ValBurnedSinceLastVesting::<T>::put(0);

                consumed_weight += T::DbWeight::get().reads_writes(3, 3);
            }

            if (now % T::UPDATE_FREQUENCY).is_zero() {
                let total_rewards = TotalValRewards::<T>::get();
                if total_rewards == 0 {
                    return consumed_weight + T::DbWeight::get().reads(1);
                }

                let current_claimable = CurrentClaimableVal::<T>::get();
                if current_claimable == 0 {
                    return consumed_weight + T::DbWeight::get().reads(1);
                }
                consumed_weight += T::DbWeight::get().reads(2);

                let batch_index: u32 =
                    ((now % T::BLOCKS_PER_DAY) / T::UPDATE_FREQUENCY).unique_saturated_into();
                if let Ok(addresses) = EthAddresses::<T>::try_get(batch_index) {
                    let wrapped_current_claimable = FixedWrapper::from(current_claimable);
                    let wrapped_total_rewards = FixedWrapper::from(total_rewards);

                    let coeff = wrapped_current_claimable / wrapped_total_rewards;

                    addresses.iter().for_each(|addr| {
                        let RewardInfo { claimable, total } = ValOwners::<T>::get(addr);
                        let amount = (FixedWrapper::from(total) * coeff.clone())
                            .try_into_balance()
                            .unwrap_or(0);
                        let new_claimable = total.min(claimable.saturating_add(amount));
                        let amount = new_claimable - claimable;
                        ValOwners::<T>::mutate(addr, |v| {
                            *v = RewardInfo::new(new_claimable, total);
                        });
                        TotalClaimableVal::<T>::mutate(|v| *v = v.saturating_add(amount));
                        consumed_weight += T::DbWeight::get().reads_writes(2, 1);
                    });
                };
            }

            consumed_weight
        }

        fn on_runtime_upgrade() -> Weight {
            match Self::storage_version() {
                Some(PalletVersion {
                    major: 1, minor: 1, ..
                }) => migrations::v1_2::migrate::<T>(),
                _ => T::DbWeight::get().reads(1),
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(WeightInfoOf::<T>::claim())]
        #[transactional]
        pub fn claim(origin: OriginFor<T>, signature: Vec<u8>) -> DispatchResultWithPostInfo {
            let account_id = ensure_signed(origin)?;
            ensure!(
                signature.len() == SIGNATURE_SIZE + 1,
                Error::<T>::SignatureInvalid
            );
            let recovery_id = if signature[SIGNATURE_SIZE] >= 27 {
                signature[SIGNATURE_SIZE] - 27
            } else {
                signature[SIGNATURE_SIZE]
            };
            let recovery_id = RecoveryId::parse(recovery_id)
                .map_err(|_| Error::<T>::SignatureVerificationFailed)?;
            let signature = Signature::parse_slice(&signature[..SIGNATURE_SIZE])
                .map_err(|_| Error::<T>::SignatureInvalid)?;
            let message = eth::prepare_message(&account_id.encode());
            let public_key = secp256k1::recover(&message, &signature, &recovery_id)
                .map_err(|_| Error::<T>::SignatureVerificationFailed)?;
            let eth_address = eth::public_key_to_eth_address(&public_key);
            let reserves_acc = ReservesAcc::<T>::get();
            let mut claimed = false;
            let mut is_eligible = false;
            Self::claim_val_reward(
                &eth_address,
                &account_id,
                &VAL.into(),
                &reserves_acc,
                &mut claimed,
                &mut is_eligible,
            )?;
            Self::claim_reward::<PswapFarmOwners<T>>(
                &eth_address,
                &account_id,
                &PSWAP.into(),
                &reserves_acc,
                &mut claimed,
                &mut is_eligible,
            )?;
            Self::claim_reward::<PswapWaifuOwners<T>>(
                &eth_address,
                &account_id,
                &PSWAP.into(),
                &reserves_acc,
                &mut claimed,
                &mut is_eligible,
            )?;
            if claimed {
                Self::deposit_event(Event::<T>::Claimed(account_id));
                Ok(().into())
            } else if is_eligible {
                Err(Error::<T>::NothingToClaim.into())
            } else {
                Err(Error::<T>::AddressNotEligible.into())
            }
        }

        /// Finalize the update of unclaimed VAL data in storage
        #[pallet::weight(WeightInfoOf::<T>::finalize_storage_migration(amounts.len() as u32))]
        #[transactional]
        pub fn finalize_storage_migration(
            origin: OriginFor<T>,
            amounts: Vec<(EthereumAddress, Balance)>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            // Ensure this call is allowed
            if MigrationPending::<T>::get() {
                migrations::v1_2::update_val_owners::<T>(amounts);
                Self::deposit_event(Event::<T>::MigrationCompleted);
                Ok(Pays::No.into())
            } else {
                Err(Error::<T>::IllegalCall.into())
            }
        }
    }

    #[pallet::event]
    #[pallet::metadata(AccountIdOf<T> = "AccountId")]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// The account has claimed their rewards. [account]
        Claimed(AccountIdOf<T>),
        /// Storage migration to version 1.2.0 completed
        MigrationCompleted,
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The account has no claimable rewards at the time of claiming request
        NothingToClaim,
        /// Address is not eligible for any rewards
        AddressNotEligible,
        /// The signature is invalid
        SignatureInvalid,
        /// The signature verification failed
        SignatureVerificationFailed,
        /// Occurs if an attempt to repeat the unclaimed VAL data update is made
        IllegalCall,
    }

    #[pallet::storage]
    pub type ReservesAcc<T: Config> = StorageValue<_, T::TechAccountId, ValueQuery>;

    /// A map EthAddresses -> RewardInfo, that is claimable and remaining vested amounts per address
    #[pallet::storage]
    pub type ValOwners<T: Config> =
        StorageMap<_, Identity, EthereumAddress, RewardInfo, ValueQuery>;

    #[pallet::storage]
    pub type PswapFarmOwners<T: Config> =
        StorageMap<_, Identity, EthereumAddress, Balance, ValueQuery>;

    #[pallet::storage]
    pub type PswapWaifuOwners<T: Config> =
        StorageMap<_, Identity, EthereumAddress, Balance, ValueQuery>;

    /// Amount of VAL burned since last vesting
    #[pallet::storage]
    pub type ValBurnedSinceLastVesting<T: Config> = StorageValue<_, Balance, ValueQuery>;

    /// Amount of VAL currently being vested (aggregated over the previous period of 14,400 blocks)
    #[pallet::storage]
    pub type CurrentClaimableVal<T: Config> = StorageValue<_, Balance, ValueQuery>;

    /// All addresses are split in batches, `AddressBatches` maps batch number to a set of addresses
    #[pallet::storage]
    pub type EthAddresses<T: Config> =
        StorageMap<_, Identity, u32, Vec<EthereumAddress>, ValueQuery>;

    /// Total amount of VAL rewards either claimable now or some time in the future
    #[pallet::storage]
    pub type TotalValRewards<T: Config> = StorageValue<_, Balance, ValueQuery>;

    /// Total amount of VAL that can be claimed by users at current point in time
    #[pallet::storage]
    pub type TotalClaimableVal<T: Config> = StorageValue<_, Balance, ValueQuery>;

    /// A flag indicating whether VAL rewards data migration has been finalized
    #[pallet::storage]
    pub type MigrationPending<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub reserves_account_id: T::TechAccountId,
        pub val_owners: Vec<(EthereumAddress, RewardInfo)>,
        pub pswap_farm_owners: Vec<(EthereumAddress, Balance)>,
        pub pswap_waifu_owners: Vec<(EthereumAddress, Balance)>,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                reserves_account_id: Default::default(),
                val_owners: Default::default(),
                pswap_farm_owners: Default::default(),
                pswap_waifu_owners: Default::default(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            ReservesAcc::<T>::put(&self.reserves_account_id);

            // Split the addresses in groups to avoid updating all rewards within a single block
            let mut iter = self.val_owners.chunks(T::MAX_CHUNK_SIZE);
            let mut batch_index: u32 = 0;
            while let Some(chunk) = iter.next() {
                EthAddresses::<T>::insert(
                    batch_index,
                    chunk
                        .iter()
                        .cloned()
                        .map(|(addr, _)| addr)
                        .collect::<Vec<_>>(),
                );
                batch_index += 1;
            }

            let mut total = balance!(0);
            let mut claimable = balance!(0);
            self.val_owners.iter().for_each(|(owner, value)| {
                ValOwners::<T>::insert(owner, value);
                claimable = claimable.saturating_add(value.claimable);
                total = total.saturating_add(value.total);
            });
            TotalValRewards::<T>::put(total);
            TotalClaimableVal::<T>::put(claimable);
            CurrentClaimableVal::<T>::put(balance!(0));
            ValBurnedSinceLastVesting::<T>::put(balance!(0));

            self.pswap_farm_owners.iter().for_each(|(owner, balance)| {
                PswapFarmOwners::<T>::insert(owner, balance);
            });
            self.pswap_waifu_owners.iter().for_each(|(owner, balance)| {
                PswapWaifuOwners::<T>::insert(owner, balance);
            });
        }
    }
}
