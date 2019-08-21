use crate::utils::wait_until;
use crate::{Net, Spec};
use ckb_app_config::CKBAppConfig;
use ckb_shared::fee_rate::FeeRate;
use ckb_types::{
    core::{Capacity, TransactionView},
    packed,
    prelude::*,
};
use log::info;

pub struct TransactionRelayLowFeeRate;

impl Spec for TransactionRelayLowFeeRate {
    crate::name!("transaction_relay_low_fee_rate");

    crate::setup!(num_nodes: 3);

    fn run(&self, net: Net) {
        net.exit_ibd_mode();

        let node0 = &net.nodes[0];
        let node1 = &net.nodes[1];
        let node2 = &net.nodes[2];

        info!("Generate new transaction on node1");
        node1.generate_block();
        let hash = node1.generate_transaction();
        let ret = wait_until(10, || {
            node1.rpc_client().get_transaction(hash.clone()).is_some()
        });
        assert!(ret, "send tx should success");
        let tx: TransactionView = packed::Transaction::from(
            node1
                .rpc_client()
                .get_transaction(hash.clone())
                .unwrap()
                .transaction
                .inner,
        )
        .into_view();
        let capacity = tx.outputs_capacity().unwrap();

        info!("Generate zero fee rate tx");
        let tx_low_fee = node1.new_transaction(hash.clone());
        // Set to zero fee
        let output = tx_low_fee
            .outputs()
            .get(0)
            .unwrap()
            .as_builder()
            .capacity(capacity.pack())
            .build();
        let tx_low_fee = tx_low_fee
            .data()
            .as_advanced_builder()
            .set_outputs(vec![])
            .output(output)
            .build();

        info!("Get tx cycles");
        let cycles = node1
            .rpc_client()
            .dry_run_transaction(tx_low_fee.data().into())
            .unwrap()
            .cycles;

        info!("Broadcast zero fee tx");
        let hash = node1
            .rpc_client()
            .broadcast_transaction(tx_low_fee.data().into(), cycles)
            .unwrap();

        info!("Waiting for relay");
        let rpc_client = node0.rpc_client();
        let ret = wait_until(1, || rpc_client.get_transaction(hash.clone()).is_some());
        assert!(!ret, "Transaction should not be relayed to node0");

        let rpc_client = node2.rpc_client();
        let ret = wait_until(1, || rpc_client.get_transaction(hash.clone()).is_some());
        assert!(!ret, "Transaction should not be relayed to node2");
    }

    fn modify_ckb_config(&self) -> Box<dyn Fn(&mut CKBAppConfig) -> ()> {
        Box::new(|config| {
            config.tx_pool.min_fee_rate = FeeRate::new(Capacity::shannons(1000));
        })
    }
}
