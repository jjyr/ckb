use super::*;
use crate::utils::is_committed;
use crate::{Net, Spec, DEFAULT_TX_PROPOSAL_WINDOW};
use ckb_chain_spec::{ChainSpec, IssuedCell};
use ckb_crypto::secp::{Generator, Privkey, Pubkey};
use ckb_dao_utils::extract_dao_data;
use ckb_test_chain_utils::always_success_cell;
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, Ratio},
    h160,
    prelude::*,
    H160,
};

const SATOSHI_CELL_CAPACITY: Capacity = Capacity::shannons(10_000_000_000_000_000);
const SATOSHI_PUBKEY_HASH: H160 = h160!("0x62e907b15cbf27d5425399ebf6f0fb50ebb88f18");
const CELLBASE_USED_BYTES: usize = 41;

pub struct DAOWithSatoshiCellOccupied;

impl Spec for DAOWithSatoshiCellOccupied {
    crate::name!("dao_with_satoshi_cell_occupied");

    fn run(&self, net: &mut Net) {
        let node0 = &net.nodes[0];
        // try deposit then withdraw dao
        node0.generate_blocks((DEFAULT_TX_PROPOSAL_WINDOW.1 + 2) as usize);
        let deposited = {
            let transaction = deposit_dao_transaction(node0);
            ensure_committed(node0, &transaction)
        };
        let transaction = withdraw_dao_transaction(node0, deposited.0.clone(), deposited.1.clone());
        node0.generate_blocks(20);
        let tx_hash = node0
            .rpc_client()
            .send_transaction(transaction.data().into());
        node0.generate_blocks(3);
        let tx_status = node0
            .rpc_client()
            .get_transaction(tx_hash.clone())
            .expect("get sent transaction");
        assert!(
            is_committed(&tx_status),
            "ensure_committed failed {:#x}",
            tx_hash
        );
    }

    fn modify_chain_spec(&self) -> Box<dyn Fn(&mut ChainSpec) -> ()> {
        Box::new(|spec_config| {
            let satoshi_cell = issue_satoshi_cell();
            spec_config.genesis.issued_cells.push(satoshi_cell);
        })
    }
}

pub struct SpendSatoshiCell {
    privkey: Privkey,
    pubkey: Pubkey,
    satoshi_cell_occupied_ratio: Ratio,
}

impl Default for SpendSatoshiCell {
    fn default() -> Self {
        Self::new()
    }
}

impl SpendSatoshiCell {
    pub fn new() -> Self {
        let (privkey, pubkey) = Generator::random_keypair();
        let satoshi_cell_occupied_ratio = Ratio(6, 10);

        SpendSatoshiCell {
            privkey,
            pubkey,
            satoshi_cell_occupied_ratio,
        }
    }
}

impl Spec for SpendSatoshiCell {
    crate::name!("spend_satoshi_cell");

    fn run(&self, net: &mut Net) {
        let node0 = &net.nodes[0];
        let satoshi_cell_occupied = SATOSHI_CELL_CAPACITY
            .safe_mul_ratio(node0.consensus().satoshi_cell_occupied_ratio)
            .unwrap();
        // check genesis blocks dao
        let genesis = node0.get_block_by_number(0);
        let (_ar, _c, _s, u) = extract_dao_data(genesis.header().dao()).expect("extract dao");
        // u - used capacity should includes virtual occupied
        assert!(u > satoshi_cell_occupied);

        // Build tx to spent virtual occupied capacity
        let cellbase = &genesis.transactions()[0];
        let satoshi_input = CellInput::new(
            OutPoint::new(cellbase.hash(), (cellbase.outputs().len() - 1) as u32),
            0,
        );
        let output = CellOutput::new_builder()
            .capacity(satoshi_cell_occupied.pack())
            .lock(always_success_cell().2.clone())
            .build();

        let transaction = TransactionBuilder::default()
            .cell_deps(vec![node0.always_success_cell_dep()])
            .input(satoshi_input)
            .output(output)
            .output_data(Bytes::new().pack())
            .build();
        let tx_hash = transaction.hash();
        let sig = self
            .privkey
            .sign_recoverable(&tx_hash.unpack())
            .expect("sign");
        let mut witness = Bytes::from(sig.serialize());
        witness.extend_from_slice(&self.pubkey.serialize());
        let transaction = transaction
            .as_advanced_builder()
            .witness(witness.pack())
            .build();

        node0.generate_blocks(1);
        let tx_hash = node0
            .rpc_client()
            .send_transaction(transaction.data().into());
        node0.generate_blocks(3);
        // cellbase occupied capacity minus satoshi cell
        let cellbase_used_capacity = Capacity::bytes(CELLBASE_USED_BYTES).unwrap();
        let tx_status = node0
            .rpc_client()
            .get_transaction(tx_hash.clone())
            .expect("get sent transaction");
        assert!(
            is_committed(&tx_status),
            "ensure_committed failed {:#x}",
            tx_hash
        );
        let tip = node0.get_tip_block();
        // check tip dao, expect u correct
        let (_ar, _c, _s, new_u) = extract_dao_data(tip.header().dao()).expect("extract dao");
        assert_eq!(
            Ok(new_u),
            u.safe_sub(satoshi_cell_occupied)
                .and_then(|c| c.safe_add(cellbase_used_capacity))
        );
    }

    fn modify_chain_spec(&self) -> Box<dyn Fn(&mut ChainSpec) -> ()> {
        let satoshi_cell_occupied_ratio = self.satoshi_cell_occupied_ratio;
        Box::new(move |spec_config| {
            spec_config.genesis.issued_cells.push(issue_satoshi_cell());
            spec_config.genesis.satoshi_gift.satoshi_cell_occupied_ratio =
                satoshi_cell_occupied_ratio;
        })
    }
}

fn issue_satoshi_cell() -> IssuedCell {
    let lock = always_success_cell()
        .2
        .clone()
        .as_builder()
        .args(Bytes::from(&SATOSHI_PUBKEY_HASH.0[..]).pack())
        .build();
    IssuedCell {
        capacity: SATOSHI_CELL_CAPACITY,
        lock: lock.into(),
    }
}
