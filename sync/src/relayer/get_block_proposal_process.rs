use crate::relayer::Relayer;
use ckb_logger::debug_target;
use ckb_network::{CKBProtocolContext, PeerIndex};
use ckb_types::{packed, prelude::*};
use failure::Error as FailureError;
use std::sync::Arc;

pub struct GetBlockProposalProcess<'a> {
    message: packed::GetBlockProposalReader<'a>,
    relayer: &'a Relayer,
    nc: Arc<dyn CKBProtocolContext>,
    peer: PeerIndex,
}

impl<'a> GetBlockProposalProcess<'a> {
    pub fn new(
        message: packed::GetBlockProposalReader<'a>,
        relayer: &'a Relayer,
        nc: Arc<dyn CKBProtocolContext>,
        peer: PeerIndex,
    ) -> Self {
        GetBlockProposalProcess {
            message,
            nc,
            relayer,
            peer,
        }
    }

    pub fn execute(self) -> Result<(), FailureError> {
        let proposals: Vec<packed::ProposalShortId> =
            self.message.proposals().to_entity().into_iter().collect();

        let fetched_transactions = {
            let tx_pool = self.relayer.shared.shared().tx_pool_controller();
            let fetch_txs = tx_pool.fetch_txs(proposals.clone());
            if let Err(e) = fetch_txs {
                debug_target!(
                    crate::LOG_TARGET_RELAY,
                    "relayer tx_pool_controller send fetch_txs error: {:?}",
                    e
                );
                return Ok(());
            }
            fetch_txs.unwrap()
        };
        let fresh_proposals: Vec<packed::ProposalShortId> = proposals
            .into_iter()
            .filter(|short_id| fetched_transactions.get(&short_id).is_none())
            .collect();

        self.relayer
            .shared()
            .state()
            .insert_get_block_proposals(self.peer, fresh_proposals);

        let content = packed::BlockProposal::new_builder()
            .transactions(
                fetched_transactions
                    .into_iter()
                    .map(|(_, tx)| tx.data())
                    .pack(),
            )
            .build();
        let message = packed::RelayMessage::new_builder().set(content).build();
        let data = message.as_slice().into();

        if let Err(err) = self.nc.send_message_to(self.peer, data) {
            debug_target!(
                crate::LOG_TARGET_RELAY,
                "relayer send GetBlockProposal error: {:?}",
                err
            );
        }
        Ok(())
    }
}
