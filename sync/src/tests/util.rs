use crate::SyncSharedState;
use ckb_chain::chain::{ChainController, ChainService};
use ckb_core::block::BlockBuilder;
use ckb_core::cell::resolve_transaction;
use ckb_core::header::HeaderBuilder;
use ckb_core::transaction::Transaction;
use ckb_core::BlockNumber;
use ckb_dao::DaoCalculator;
use ckb_db::MemoryKeyValueDB;
use ckb_merkle_mountain_range::{leaf_index_to_mmr_size, MMRBatch, MMR};
use ckb_notify::NotifyService;
use ckb_shared::shared::{Shared, SharedBuilder};
use ckb_store::{ChainKVStore, ChainStore, MMRStoreWrapper};
use ckb_test_chain_utils::{always_success_cellbase, always_success_consensus};
use ckb_traits::ChainProvider;
use numext_fixed_hash::H256;
use std::sync::Arc;

pub fn build_chain(
    tip: BlockNumber,
) -> (
    SyncSharedState<ChainKVStore<MemoryKeyValueDB>>,
    ChainController,
) {
    let shared = SharedBuilder::<MemoryKeyValueDB>::new()
        .consensus(always_success_consensus())
        .build()
        .unwrap();
    let chain_controller = {
        let notify_controller = NotifyService::default().start::<&str>(None);
        let chain_service = ChainService::new(shared.clone(), notify_controller);
        chain_service.start::<&str>(None)
    };
    generate_blocks(&shared, &chain_controller, tip);
    let sync_shared_state = SyncSharedState::new(shared);
    (sync_shared_state, chain_controller)
}

pub fn generate_blocks(
    shared: &Shared<ChainKVStore<MemoryKeyValueDB>>,
    chain_controller: &ChainController,
    target_tip: BlockNumber,
) {
    let parent_number = shared.lock_chain_state().tip_number();
    let mut parent_hash = shared.lock_chain_state().tip_hash().clone();

    let mut mmr_batch = MMRBatch::new();
    let mmr_store = MMRStoreWrapper::new(Arc::clone(shared.store()));
    let mmr_size = leaf_index_to_mmr_size(parent_number);
    let mut mmr = MMR::new(mmr_size, mmr_store);
    for _block_number in parent_number + 1..=target_tip {
        let root = mmr
            .get_root(Some(&mmr_batch))
            .expect("get root")
            .expect("must exists");
        let chain_commitment = root.hash();
        let block = inherit_block(shared, &parent_hash, chain_commitment).build();
        let header = block.header().to_owned();
        parent_hash = header.hash().to_owned();
        chain_controller
            .process_block(Arc::new(block), false)
            .expect("processing block should be ok");
        mmr.push(&mut mmr_batch, (&header).into())
            .expect("mmr push");
    }
}

pub fn inherit_block(
    shared: &Shared<ChainKVStore<MemoryKeyValueDB>>,
    parent_hash: &H256,
    chain_commitment: &H256,
) -> BlockBuilder {
    let parent = shared.store().get_block(parent_hash).unwrap();
    let parent_epoch = shared.get_block_epoch(parent_hash).unwrap();
    let parent_number = parent.header().number();
    let epoch = shared
        .next_epoch_ext(&parent_epoch, parent.header())
        .unwrap_or(parent_epoch);
    let cellbase = {
        let (_, reward) = shared.finalize_block_reward(parent.header()).unwrap();
        always_success_cellbase(parent_number + 1, reward)
    };
    let dao = {
        let chain_state = shared.lock_chain_state();
        let resolved_cellbase = resolve_transaction(
            &cellbase,
            &mut Default::default(),
            &*chain_state,
            &*chain_state,
        )
        .unwrap();
        DaoCalculator::new(shared.consensus(), Arc::clone(shared.store()))
            .dao_field(&[resolved_cellbase], parent.header())
            .unwrap()
    };

    BlockBuilder::from_header_builder(
        HeaderBuilder::default()
            .parent_hash(parent_hash.to_owned())
            .number(parent.header().number() + 1)
            .timestamp(parent.header().timestamp() + 1)
            .epoch(epoch.number())
            .difficulty(epoch.difficulty().to_owned())
            .chain_commitment(chain_commitment.to_owned())
            .dao(dao),
    )
    .transaction(inherit_cellbase(shared, parent_number))
}

pub fn inherit_cellbase(
    shared: &Shared<ChainKVStore<MemoryKeyValueDB>>,
    parent_number: BlockNumber,
) -> Transaction {
    let parent_header = {
        let chain = shared.lock_chain_state();
        let parent_hash = chain
            .store()
            .get_block_hash(parent_number)
            .expect("parent exist");
        chain
            .store()
            .get_block_header(&parent_hash)
            .expect("parent exist")
    };
    let (_, reward) = shared.finalize_block_reward(&parent_header).unwrap();
    always_success_cellbase(parent_number + 1, reward)
}
