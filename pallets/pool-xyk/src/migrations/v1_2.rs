use frame_support::dispatch::Weight;
use frame_support::traits::Get;

use crate::{AccountPools, Config, PoolProviders, Properties};

pub fn migrate<T: Config>() -> Weight {
    for (_, target_asset, (pool_account, _)) in Properties::<T>::iter() {
        for (user_account, _pool_tokens_balance) in PoolProviders::<T>::iter_prefix(pool_account) {
            AccountPools::<T>::mutate(user_account, |set| set.insert(target_asset));
        }
    }
    T::BlockWeights::get().max_block
}

#[cfg(test)]
mod tests {
    use common::{balance, AssetName, AssetSymbol};
    use hex_literal::hex;

    use crate::mock::*;
    use crate::{AccountPools, PoolProviders, Properties};

    #[test]
    fn test() {
        ExtBuilder::default().build().execute_with(|| {
            let base_asset = GetBaseAssetId::get();
            let dex_id = DEX_A_ID;
            let target_asset_a = AssetId::from_bytes(
                hex!("0200000700000000000000000000000000000000000000000000000000000000").into(),
            );
            let target_asset_b = AssetId::from_bytes(
                hex!("0200010700000000000000000000000000000000000000000000000000000000").into(),
            );
            let target_asset_c = AssetId::from_bytes(
                hex!("0200020700000000000000000000000000000000000000000000000000000000").into(),
            );

            assets::Module::<Runtime>::register_asset_id(
                ALICE(),
                base_asset.clone(),
                AssetSymbol(b"BASE".to_vec()),
                AssetName(b"BASE".to_vec()),
                18,
                0,
                true,
            )
            .unwrap();
            for target_asset in [target_asset_a, target_asset_b, target_asset_c].iter() {
                assets::Module::<Runtime>::register_asset_id(
                    ALICE(),
                    target_asset.clone(),
                    AssetSymbol(b"A".to_vec()),
                    AssetName(b"B".to_vec()),
                    18,
                    0,
                    true,
                )
                .unwrap();
                trading_pair::Module::<Runtime>::register(
                    Origin::signed(ALICE()),
                    dex_id,
                    base_asset.clone(),
                    target_asset.clone(),
                )
                .unwrap();
                crate::Module::<Runtime>::initialize_pool(
                    Origin::signed(ALICE()),
                    dex_id,
                    base_asset.clone(),
                    target_asset.clone(),
                )
                .unwrap();

                let (_, tech_account) = PoolXYK::tech_account_from_dex_and_asset_pair(
                    dex_id,
                    base_asset.clone(),
                    target_asset.clone(),
                )
                .unwrap();
                let pool_account = Technical::tech_account_id_to_account_id(&tech_account).unwrap();
                // using CHARLIE() account for fee account here, because it's not used but to avoid missing bugs,
                // better be different from pool account
                Properties::<Runtime>::insert(
                    base_asset,
                    target_asset,
                    (pool_account.clone(), CHARLIE()),
                );
                for account in [ALICE(), BOB(), CHARLIE()].iter() {
                    PoolProviders::<Runtime>::insert(pool_account.clone(), account, balance!(42));
                }
            }

            super::migrate::<Runtime>();

            for account in [ALICE(), BOB(), CHARLIE()].iter() {
                assert_eq!(
                    AccountPools::<Runtime>::get(account),
                    [target_asset_a, target_asset_b, target_asset_c]
                        .iter()
                        .cloned()
                        .collect()
                )
            }
        });
    }
}
