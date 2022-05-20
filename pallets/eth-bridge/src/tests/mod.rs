use crate::offchain::SignatureParams;
use crate::requests::{
    IncomingRequest, IncomingRequestKind, OffchainRequest, OutgoingRequest, RequestStatus,
};
use crate::tests::mock::*;
use crate::util::majority;
use codec::{Decode, Encode};
use common::eth;
use frame_support::dispatch::DispatchErrorWithPostInfo;
use frame_support::weights::Pays;
use frame_support::{assert_err, assert_ok, ensure};

use secp256k1::{PublicKey, SecretKey};
use sp_core::{ecdsa, Public, H256};
use std::collections::BTreeSet;

mod asset;
mod cancel;
mod incoming_transfer;
pub mod mock;
mod ocw;
mod outgoing_tranfser;
mod peer;

pub(crate) type Error = crate::Error<Runtime>;
pub(crate) type Assets = assets::Pallet<Runtime>;

pub const ETH_NETWORK_ID: u32 = 0;

fn get_signature_params(signature: &ecdsa::Signature) -> SignatureParams {
    let encoded = signature.encode();
    let mut params = SignatureParams::decode(&mut &encoded[..]).expect("Wrong signature format");
    params.v += 27;
    params
}

pub fn last_event() -> Option<Event> {
    frame_system::Module::<Runtime>::events()
        .pop()
        .map(|x| x.event)
}

pub fn no_event() -> bool {
    frame_system::Module::<Runtime>::events().pop().is_none()
}

pub fn approve_request(
    state: &State,
    request: OutgoingRequest<Runtime>,
    request_hash: H256,
) -> Result<(), Option<Event>> {
    let encoded = request.to_eth_abi(request_hash).unwrap();
    System::reset_events();
    let net_id = request.network_id();
    let mut approvals = BTreeSet::new();
    let keypairs = &state.networks[&net_id].ocw_keypairs;
    for (i, (_signer, account_id, seed)) in keypairs.iter().enumerate() {
        let secret = SecretKey::parse_slice(seed).unwrap();
        let public = PublicKey::from_secret_key(&secret);
        let msg = eth::prepare_message(encoded.as_raw());
        let sig_pair = secp256k1::sign(&msg, &secret);
        let signature = sig_pair.into();
        let signature_params = get_signature_params(&signature);
        approvals.insert(signature_params.clone());
        let additional_sigs = if crate::PendingPeer::<Runtime>::get(net_id).is_some() {
            1
        } else {
            0
        };
        let sigs_needed = majority(keypairs.len()) + additional_sigs;
        let current_status = crate::RequestStatuses::<Runtime>::get(net_id, &request_hash).unwrap();
        ensure!(
            EthBridge::approve_request(
                Origin::signed(account_id.clone()),
                ecdsa::Public::from_slice(&public.serialize_compressed()),
                request_hash,
                signature_params,
                net_id
            )
            .is_ok(),
            None
        );
        if current_status == RequestStatus::Pending && i + 1 == sigs_needed {
            match last_event().ok_or(None)? {
                Event::eth_bridge(bridge_event) => match bridge_event {
                    crate::Event::ApprovalsCollected(h) => {
                        assert_eq!(h, request_hash);
                    }
                    e => {
                        assert_ne!(
                            crate::RequestsQueue::<Runtime>::get(net_id).last(),
                            Some(&request_hash)
                        );
                        return Err(Some(Event::eth_bridge(e)));
                    }
                },
                e => panic!("Unexpected event: {:?}", e),
            }
        } else {
            assert!(no_event());
        }
        System::reset_events();
    }
    assert_ne!(
        crate::RequestsQueue::<Runtime>::get(net_id).last(),
        Some(&request_hash)
    );
    Ok(())
}

pub fn last_request(net_id: u32) -> Option<OffchainRequest<Runtime>> {
    let request_hash = crate::RequestsQueue::<Runtime>::get(net_id)
        .last()
        .cloned()?;
    crate::Requests::<Runtime>::get(net_id, request_hash)
}

pub fn last_outgoing_request(net_id: u32) -> Option<(OutgoingRequest<Runtime>, H256)> {
    let request = last_request(net_id)?;
    match request {
        OffchainRequest::Outgoing(r, hash) => Some((r, hash)),
        _ => panic!("Unexpected request type"),
    }
}

pub fn approve_last_request(
    state: &State,
    net_id: u32,
) -> Result<(OutgoingRequest<Runtime>, H256), Option<Event>> {
    let (outgoing_request, hash) = last_outgoing_request(net_id).ok_or(None)?;
    approve_request(state, outgoing_request.clone(), hash)?;
    Ok((outgoing_request, hash))
}

pub fn approve_next_request(
    state: &State,
    net_id: u32,
) -> Result<(OutgoingRequest<Runtime>, H256), Option<Event>> {
    let request_hash = crate::RequestsQueue::<Runtime>::get(net_id).remove(0);
    let (outgoing_request, hash) = crate::Requests::<Runtime>::get(net_id, request_hash)
        .ok_or(None)?
        .into_outgoing()
        .unwrap();
    approve_request(state, outgoing_request.clone(), hash)?;
    Ok((outgoing_request, hash))
}

pub fn request_incoming(
    account_id: AccountId,
    tx_hash: H256,
    kind: IncomingRequestKind,
    net_id: u32,
) -> Result<H256, Event> {
    assert_ok!(EthBridge::request_from_sidechain(
        Origin::signed(account_id),
        tx_hash,
        kind,
        net_id
    ));
    let last_request: OffchainRequest<Runtime> = last_request(net_id).unwrap();
    match last_request {
        OffchainRequest::LoadIncoming(..) => (),
        _ => panic!("Invalid off-chain request"),
    }
    let hash = last_request.hash();
    assert_eq!(
        crate::RequestStatuses::<Runtime>::get(net_id, &hash).unwrap(),
        RequestStatus::Pending
    );
    Ok(hash)
}

pub fn assert_incoming_request_done(
    state: &State,
    incoming_request: IncomingRequest<Runtime>,
) -> Result<(), Option<Event>> {
    let net_id = incoming_request.network_id();
    let bridge_acc_id = state.networks[&net_id].config.bridge_account_id.clone();
    let sidechain_req_hash = incoming_request.hash();
    assert_eq!(
        crate::RequestsQueue::<Runtime>::get(net_id)
            .last()
            .unwrap()
            .0,
        sidechain_req_hash.0
    );
    assert_ok!(EthBridge::register_incoming_request(
        Origin::signed(bridge_acc_id.clone()),
        incoming_request.clone(),
    ));
    let req_hash = crate::LoadToIncomingRequestHash::<Runtime>::get(net_id, sidechain_req_hash);
    assert_ne!(
        crate::RequestsQueue::<Runtime>::get(net_id)
            .last()
            .map(|x| x.0),
        Some(sidechain_req_hash.0)
    );
    assert!(crate::RequestsQueue::<Runtime>::get(net_id).contains(&req_hash));
    assert_eq!(
        *crate::Requests::get(net_id, &req_hash)
            .unwrap()
            .as_incoming()
            .unwrap()
            .0,
        incoming_request
    );
    assert_ok!(EthBridge::finalize_incoming_request(
        Origin::signed(bridge_acc_id.clone()),
        req_hash,
        net_id,
    ));
    assert_eq!(
        crate::RequestStatuses::<Runtime>::get(net_id, &req_hash).unwrap(),
        RequestStatus::Done
    );
    assert!(!crate::RequestsQueue::<Runtime>::get(net_id).contains(&req_hash));
    Ok(())
}

pub fn assert_incoming_request_registration_failed(
    state: &State,
    incoming_request: IncomingRequest<Runtime>,
    error: crate::Error<Runtime>,
) -> Result<(), Event> {
    let net_id = incoming_request.network_id();
    let bridge_acc_id = state.networks[&net_id].config.bridge_account_id.clone();
    assert_eq!(
        crate::RequestsQueue::<Runtime>::get(net_id)
            .last()
            .unwrap()
            .0,
        incoming_request.hash().0
    );
    assert_err!(
        EthBridge::register_incoming_request(
            Origin::signed(bridge_acc_id.clone()),
            incoming_request.clone(),
        ),
        DispatchErrorWithPostInfo {
            post_info: Pays::No.into(),
            error: error.into()
        }
    );
    Ok(())
}
