use crate::relayer::Relayer;
use ckb_logger::{debug_target, trace_target};
use ckb_network::{CKBProtocolContext, PeerIndex};
use ckb_types::{packed, prelude::*};
use failure::Error as FailureError;
use std::sync::Arc;

pub struct GetTransactionsProcess<'a> {
    message: packed::GetRelayTransactionsReader<'a>,
    relayer: &'a Relayer,
    nc: Arc<dyn CKBProtocolContext>,
    peer: PeerIndex,
}

impl<'a> GetTransactionsProcess<'a> {
    pub fn new(
        message: packed::GetRelayTransactionsReader<'a>,
        relayer: &'a Relayer,
        nc: Arc<dyn CKBProtocolContext>,
        peer: PeerIndex,
    ) -> Self {
        GetTransactionsProcess {
            message,
            relayer,
            nc,
            peer,
        }
    }

    pub fn execute(self) -> Result<(), FailureError> {
        let tx_hashes = self.message.tx_hashes();

        trace_target!(
            crate::LOG_TARGET_RELAY,
            "{} request transactions({})",
            self.peer,
            tx_hashes
        );

        let transactions: Vec<_> = {
            let tx_pool = self.relayer.shared.shared().tx_pool_controller();

            let fetch_txs_with_cycles = tx_pool.fetch_txs_with_cycles(
                tx_hashes
                    .iter()
                    .map(|tx_hash| packed::ProposalShortId::from_tx_hash(&tx_hash.to_entity()))
                    .collect(),
            );

            if let Err(e) = fetch_txs_with_cycles {
                debug_target!(
                    crate::LOG_TARGET_RELAY,
                    "relayer tx_pool_controller send fetch_txs_with_cycles error: {:?}",
                    e,
                );
                return Ok(());
            };

            fetch_txs_with_cycles
                .unwrap()
                .into_iter()
                .map(|(_, (tx, cycles))| {
                    packed::RelayTransaction::new_builder()
                        .cycles(cycles.pack())
                        .transaction(tx.data())
                        .build()
                })
                .collect()
        };

        if !transactions.is_empty() {
            let txs = packed::RelayTransactions::new_builder()
                .transactions(transactions.pack())
                .build();
            let message = packed::RelayMessage::new_builder().set(txs).build();
            let data = message.as_slice().into();
            if let Err(err) = self.nc.send_message_to(self.peer, data) {
                debug_target!(
                    crate::LOG_TARGET_RELAY,
                    "relayer send Transactions error: {:?}",
                    err,
                );
            }
        }
        Ok(())
    }
}
