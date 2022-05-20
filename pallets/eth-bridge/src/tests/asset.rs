use super::mock::*;
use super::{Assets, Error, EthBridge};
use crate::contract::{ContractEvent, DepositEvent};
use crate::requests::{
    AssetKind, IncomingRequest, IncomingTransactionRequestKind, LoadIncomingTransactionRequest,
    OutgoingRequest,
};
use crate::tests::mock::{get_account_id_from_seed, ExtBuilder};
use crate::tests::{
    approve_last_request, assert_incoming_request_done, request_incoming, ETH_NETWORK_ID,
};
use crate::Address;
use common::{
    balance, AssetId32, AssetName, AssetSymbol, Balance, PredefinedAssetId,
    DEFAULT_BALANCE_PRECISION, XOR,
};
use frame_support::sp_runtime::app_crypto::sp_core::{self, sr25519};
use frame_support::{assert_err, assert_noop, assert_ok};
use hex_literal::hex;
use sp_core::H256;
use std::str::FromStr;

#[test]
fn should_mint_and_burn_sidechain_asset() {
    let (mut ext, state) = ExtBuilder::default().build();

    #[track_caller]
    fn check_invariant(asset_id: &AssetId32<PredefinedAssetId>, val: u32) {
        assert_eq!(Assets::total_issuance(asset_id).unwrap(), val.into());
    }

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let token_address = Address::from(hex!("7d7ff6f42e928de241282b9606c8e98ea48526e2"));
        EthBridge::register_sidechain_asset(
            token_address,
            18,
            AssetSymbol(b"TEST".to_vec()),
            AssetName(b"TEST Asset".to_vec()),
            net_id,
        )
        .unwrap();
        let (asset_id, asset_kind) =
            EthBridge::get_asset_by_raw_asset_id(H256::zero(), &token_address, net_id)
                .unwrap()
                .unwrap();
        assert_eq!(asset_kind, AssetKind::Sidechain);
        check_invariant(&asset_id, 0);
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1u8; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id,
            asset_kind,
            amount: 100u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: ETH_NETWORK_ID,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();
        check_invariant(&asset_id, 100);
        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id,
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            100_u32.into(),
            net_id,
        ));
        approve_last_request(&state, net_id).expect("request wasn't approved");
        check_invariant(&asset_id, 0);
    });
}

#[test]
fn should_not_burn_or_mint_sidechain_owned_asset() {
    let (mut ext, state) = ExtBuilder::default().build();

    #[track_caller]
    fn check_invariant() {
        assert_eq!(
            Assets::total_issuance(&XOR.into()).unwrap(),
            balance!(350000)
        );
    }

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        assert_eq!(
            EthBridge::registered_asset(net_id, AssetId32::from(XOR)).unwrap(),
            AssetKind::SidechainOwned
        );
        check_invariant();
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1u8; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id: XOR.into(),
            asset_kind: AssetKind::SidechainOwned,
            amount: 100u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: ETH_NETWORK_ID,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();
        check_invariant();
        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            XOR.into(),
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            100_u32.into(),
            net_id,
        ));
        approve_last_request(&state, net_id).expect("request wasn't approved");
        check_invariant();
    });
}

#[test]
fn should_register_and_find_asset_ids() {
    let (mut ext, _state) = ExtBuilder::default().build();
    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        // gets a known asset
        let (asset_id, asset_kind) = EthBridge::get_asset_by_raw_asset_id(
            H256(AssetId32::<PredefinedAssetId>::from_asset_id(PredefinedAssetId::XOR).code),
            &Address::zero(),
            net_id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(asset_id, XOR.into());
        assert_eq!(asset_kind, AssetKind::Thischain);
        let token_address = Address::from(hex!("7d7ff6f42e928de241282b9606c8e98ea48526e2"));
        // registers unknown token
        assert!(
            EthBridge::get_asset_by_raw_asset_id(H256::zero(), &token_address, net_id)
                .unwrap()
                .is_none()
        );
        // gets registered asset ID, associated with the token
        EthBridge::register_sidechain_asset(
            token_address,
            18,
            AssetSymbol(b"TEST".to_vec()),
            AssetName(b"TEST Asset".to_vec()),
            net_id,
        )
        .unwrap();
        let (asset_id, asset_kind) =
            EthBridge::get_asset_by_raw_asset_id(H256::zero(), &token_address, net_id)
                .unwrap()
                .unwrap();
        assert_eq!(
            asset_id,
            AssetId32::from_bytes(hex!(
                "00998577153deb622b5d7faabf23846281a8b074e1d4eebd31bca9dbe2c23006"
            ))
        );
        assert_eq!(asset_kind, AssetKind::Sidechain);
        assert_eq!(
            EthBridge::registered_sidechain_token(net_id, &asset_id).unwrap(),
            token_address
        );
        assert_eq!(
            EthBridge::registered_sidechain_asset(net_id, &token_address).unwrap(),
            asset_id
        );
    });
}

#[test]
fn should_add_asset() {
    let (mut ext, state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let asset_id = Assets::register_from(
            &alice,
            AssetSymbol(b"TEST".to_vec()),
            AssetName(b"TEST Asset".to_vec()),
            18,
            Balance::from(0u32),
            true,
        )
        .unwrap();
        assert_ok!(EthBridge::add_asset(
            Origin::signed(alice.clone()),
            asset_id,
            net_id,
        ));
        assert!(EthBridge::registered_asset(net_id, asset_id).is_none());
        approve_last_request(&state, net_id).expect("request wasn't approved");
        assert_eq!(
            EthBridge::registered_asset(net_id, asset_id).unwrap(),
            AssetKind::Thischain
        );
    });
}

#[test]
fn should_add_token() {
    let (mut ext, state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let symbol = "TEST".into();
        let name = "Runtime Token".into();
        let decimals = 18;
        assert_ok!(EthBridge::add_sidechain_token(
            Origin::root(),
            token_address,
            symbol,
            name,
            decimals,
            ETH_NETWORK_ID,
        ));
        assert!(EthBridge::registered_sidechain_asset(net_id, &token_address).is_none());
        approve_last_request(&state, net_id).expect("request wasn't approved");
        let asset_id_opt = EthBridge::registered_sidechain_asset(net_id, &token_address);
        assert!(asset_id_opt.is_some());
        assert_eq!(
            EthBridge::registered_asset(net_id, asset_id_opt.unwrap()).unwrap(),
            AssetKind::Sidechain
        );
    });
}

#[ignore]
#[test]
fn should_not_add_token_if_not_bridge_account() {
    let (mut ext, _state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let bob = get_account_id_from_seed::<sr25519::Public>("Bob");
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let symbol = "TEST".into();
        let name = "Runtime Token".into();
        let decimals = 18;
        assert_err!(
            EthBridge::add_sidechain_token(
                Origin::signed(bob),
                token_address,
                symbol,
                name,
                decimals,
                net_id,
            ),
            Error::Forbidden
        );
    });
}

#[test]
fn should_reserve_owned_asset_on_different_networks() {
    let mut builder = ExtBuilder::default();
    let net_id_0 = ETH_NETWORK_ID;
    let net_id_1 = builder.add_network(vec![], None, None);
    let (mut ext, state) = builder.build();

    ext.execute_with(|| {
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let asset_id = XOR.into();
        Assets::mint_to(&asset_id, &alice, &alice, 100u32.into()).unwrap();
        Assets::mint_to(
            &asset_id,
            &alice,
            &state.networks[&net_id_0].config.bridge_account_id,
            100u32.into(),
        )
        .unwrap();
        Assets::mint_to(
            &asset_id,
            &alice,
            &state.networks[&net_id_1].config.bridge_account_id,
            100u32.into(),
        )
        .unwrap();
        let supply = Assets::total_issuance(&asset_id).unwrap();
        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id,
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            50_u32.into(),
            net_id_0,
        ));
        approve_last_request(&state, net_id_0).expect("request wasn't approved");
        assert_ok!(EthBridge::add_asset(
            Origin::signed(alice.clone()),
            asset_id,
            net_id_1,
        ));
        approve_last_request(&state, net_id_1).expect("request wasn't approved");
        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id,
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            50_u32.into(),
            net_id_1,
        ));
        approve_last_request(&state, net_id_1).expect("request wasn't approved");
        assert_eq!(Assets::total_issuance(&asset_id).unwrap(), supply);

        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1u8; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id_0,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id,
            asset_kind: AssetKind::Thischain,
            amount: 50u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: net_id_0,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[2; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id_1,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id,
            asset_kind: AssetKind::Thischain,
            amount: 50u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: net_id_1,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();
        assert_eq!(Assets::total_issuance(&asset_id).unwrap(), supply);
    });
}

#[test]
fn should_handle_sidechain_and_thischain_asset_on_different_networks() {
    let mut builder = ExtBuilder::default();
    let net_id_0 = ETH_NETWORK_ID;
    let net_id_1 = builder.add_network(vec![], None, None);
    let (mut ext, state) = builder.build();

    ext.execute_with(|| {
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        // Register token on the first network.
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        assert_ok!(EthBridge::add_sidechain_token(
            Origin::root(),
            token_address,
            "TEST".into(),
            "Runtime Token".into(),
            18,
            net_id_0,
        ));
        approve_last_request(&state, net_id_0).expect("request wasn't approved");
        let asset_id = EthBridge::registered_sidechain_asset(net_id_0, &token_address)
            .expect("Asset wasn't found.");
        assert_eq!(
            EthBridge::registered_asset(net_id_0, asset_id).unwrap(),
            AssetKind::Sidechain
        );

        // Register the newly generated asset in the second network
        assert_ok!(EthBridge::add_asset(
            Origin::signed(alice.clone()),
            asset_id,
            net_id_1,
        ));
        approve_last_request(&state, net_id_1).expect("request wasn't approved");
        assert_eq!(
            EthBridge::registered_asset(net_id_1, asset_id).unwrap(),
            AssetKind::Thischain
        );
        Assets::mint_to(
            &asset_id,
            &state.networks[&net_id_0].config.bridge_account_id,
            &state.networks[&net_id_1].config.bridge_account_id,
            100u32.into(),
        )
        .unwrap();
        let supply = Assets::total_issuance(&asset_id).unwrap();
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1u8; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id_0,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id,
            asset_kind: AssetKind::Sidechain,
            amount: 50u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: net_id_0,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();

        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id,
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            50_u32.into(),
            net_id_1,
        ));
        approve_last_request(&state, net_id_1).expect("request wasn't approved");

        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[2; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id_1,
        )
        .unwrap();
        let incoming_transfer = IncomingRequest::Transfer(crate::IncomingTransfer {
            from: Address::from([1; 20]),
            to: alice.clone(),
            asset_id,
            asset_kind: AssetKind::Thischain,
            amount: 50u32.into(),
            author: alice.clone(),
            tx_hash,
            at_height: 1,
            timepoint: Default::default(),
            network_id: net_id_1,
            should_take_fee: false,
        });
        assert_incoming_request_done(&state, incoming_transfer.clone()).unwrap();

        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id,
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            50_u32.into(),
            net_id_0,
        ));
        approve_last_request(&state, net_id_0).expect("request wasn't approved");
        assert_eq!(Assets::total_issuance(&asset_id).unwrap(), supply);
    });
}

#[test]
fn should_convert_amount_for_a_token_with_non_default_precision() {
    let (mut ext, state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let ticker = "USDT".into();
        let name = "Tether USD".into();
        let decimals = 6;
        assert_ok!(EthBridge::add_sidechain_token(
            Origin::root(),
            token_address,
            ticker,
            name,
            decimals,
            net_id,
        ));
        assert!(EthBridge::registered_sidechain_asset(net_id, &token_address).is_none());
        approve_last_request(&state, net_id).expect("request wasn't approved");
        let asset_id = EthBridge::registered_sidechain_asset(net_id, &token_address)
            .expect("failed to register sidechain asset");
        assert_eq!(
            EthBridge::registered_asset(net_id, &asset_id).unwrap(),
            AssetKind::Sidechain
        );
        assert_eq!(
            EthBridge::sidechain_asset_precision(net_id, &asset_id),
            decimals
        );
        assert_eq!(
            Assets::get_asset_info(&asset_id).2,
            DEFAULT_BALANCE_PRECISION
        );
        // Incoming transfer part.
        assert_eq!(
            Assets::total_balance(&asset_id, &alice).unwrap(),
            balance!(0)
        );
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id,
        )
        .unwrap();
        let sidechain_amount = 1 * 10_u128.pow(decimals as u32);
        let incoming_trasfer = IncomingRequest::try_from_contract_event(
            ContractEvent::Deposit(DepositEvent::new(
                alice.clone(),
                sidechain_amount,
                token_address,
                H256::zero(),
            )),
            LoadIncomingTransactionRequest::new(
                alice.clone(),
                tx_hash,
                Default::default(),
                IncomingTransactionRequestKind::Transfer,
                net_id,
            ),
            1,
        )
        .unwrap();
        assert_incoming_request_done(&state, incoming_trasfer).unwrap();
        assert_eq!(
            Assets::total_balance(&asset_id, &alice).unwrap(),
            balance!(1)
        );
        // Outgoing transfer part.
        assert_ok!(EthBridge::transfer_to_sidechain(
            Origin::signed(alice.clone()),
            asset_id.clone(),
            Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
            balance!(1),
            net_id,
        ));
        let outgoing_transfer =
            match approve_last_request(&state, net_id).expect("request wasn't approved") {
                (OutgoingRequest::Transfer(transfer), _) => transfer,
                _ => unreachable!(),
            };
        assert_eq!(outgoing_transfer.amount, balance!(1));
        assert_eq!(
            outgoing_transfer.sidechain_amount().unwrap().0,
            sidechain_amount
        );
        assert_eq!(
            Assets::total_balance(&asset_id, &alice).unwrap(),
            balance!(0)
        );
    });
}

#[test]
fn should_fail_convert_amount_for_a_token_with_non_default_precision() {
    let (mut ext, state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let ticker = "USDT".into();
        let name = "Tether USD".into();
        let decimals = 6;
        assert_ok!(EthBridge::add_sidechain_token(
            Origin::root(),
            token_address,
            ticker,
            name,
            decimals,
            net_id,
        ));
        assert!(EthBridge::registered_sidechain_asset(net_id, &token_address).is_none());
        approve_last_request(&state, net_id).expect("request wasn't approved");
        let asset_id = EthBridge::registered_sidechain_asset(net_id, &token_address)
            .expect("failed to register sidechain asset");
        assert_eq!(
            Assets::total_balance(&asset_id, &alice).unwrap(),
            balance!(0)
        );
        let tx_hash = request_incoming(
            alice.clone(),
            H256::from_slice(&[1; 32]),
            IncomingTransactionRequestKind::Transfer.into(),
            net_id,
        )
        .unwrap();
        let sidechain_amount = 1_000_000_000_000_000_000_000 * 10_u128.pow(decimals as u32);
        let incoming_trasfer_result = IncomingRequest::try_from_contract_event(
            ContractEvent::Deposit(DepositEvent::new(
                alice.clone(),
                sidechain_amount,
                token_address,
                H256::zero(),
            )),
            LoadIncomingTransactionRequest::new(
                alice.clone(),
                tx_hash,
                Default::default(),
                IncomingTransactionRequestKind::Transfer,
                net_id,
            ),
            1,
        );
        assert_eq!(
            incoming_trasfer_result,
            Err(Error::UnsupportedAssetPrecision)
        );
    });
}

#[test]
fn should_fail_tranfer_amount_with_dust_for_a_token_with_non_default_precision() {
    let (mut ext, state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let alice = get_account_id_from_seed::<sr25519::Public>("Alice");
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let ticker = "USDT".into();
        let name = "Tether USD".into();
        let decimals = 6;
        assert_ok!(EthBridge::add_sidechain_token(
            Origin::root(),
            token_address,
            ticker,
            name,
            decimals,
            net_id,
        ));
        assert!(EthBridge::registered_sidechain_asset(net_id, &token_address).is_none());
        approve_last_request(&state, net_id).expect("request wasn't approved");
        let asset_id = EthBridge::registered_sidechain_asset(net_id, &token_address)
            .expect("failed to register sidechain asset");
        assert_eq!(
            Assets::total_balance(&asset_id, &alice).unwrap(),
            balance!(0)
        );
        Assets::mint_to(
            &asset_id,
            &state.networks[&net_id].config.bridge_account_id,
            &alice,
            balance!(0.1000009),
        )
        .unwrap();
        assert_noop!(
            EthBridge::transfer_to_sidechain(
                Origin::signed(alice.clone()),
                asset_id.clone(),
                Address::from_str("19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A").unwrap(),
                balance!(0.1000009),
                net_id,
            ),
            Error::NonZeroDust
        );
    });
}

#[test]
fn should_not_allow_registering_sidechain_token_with_big_precision() {
    let (mut ext, _state) = ExtBuilder::default().build();

    ext.execute_with(|| {
        let net_id = ETH_NETWORK_ID;
        let token_address = Address::from(hex!("e88f8313e61a97cec1871ee37fbbe2a8bf3ed1e4"));
        let ticker = "USDT".into();
        let name = "Tether USD".into();
        let decimals = DEFAULT_BALANCE_PRECISION + 1;
        assert_noop!(
            EthBridge::add_sidechain_token(
                Origin::root(),
                token_address,
                ticker,
                name,
                decimals,
                net_id,
            ),
            Error::UnsupportedAssetPrecision
        );
    });
}
