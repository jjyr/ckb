use crate::fee_rate::FeeRate;
use crate::tx_confirm_stat::TxConfirmStat;
use ckb_core::{Capacity, Cycle};
use fnv::FnvHashMap;
use numext_fixed_hash::H256;

const DEFAULT_CYCLES_PER_BYTE: u64 = 2000;
const MIN_BUCKET_FEERATE: f64 = 1000f64;
const MAX_BUCKET_FEERATE: f64 = 1e7;
const FEE_SPACING: f64 = 1.05f64;
const MAX_CONFIRM_BLOCKS: usize = 1000;
const MIN_ESTIMATE_SAMPLES: usize = 20;
const MIN_ESTIMATE_CONFIRM_RATE: f64 = 0.85f64;
/// half life each 100 blocks
const DEFAULT_DECAY_FACTOR: f64 = 0.993;

struct TxRecord {
    height: u64,
    bucket_index: usize,
}

pub struct TxEntry {
    hash: H256,
    height: u64,
    cycles: Cycle,
    fee: Capacity,
    size: usize,
}

impl TxEntry {
    fn hash(&self) -> &H256 {
        &self.hash
    }

    fn virtual_size(&self) -> usize {
        std::cmp::max(self.size, (self.cycles / DEFAULT_CYCLES_PER_BYTE) as usize)
    }

    fn feerate(&self) -> FeeRate {
        FeeRate::from_f64(self.fee.as_u64() as f64 / self.virtual_size() as f64)
            .expect("tx feerate")
    }
}

/// Fee Estimator
/// Estimator track new_block and tx_pool to collect data
/// we track every new tx enter txpool and record the tip height and feerate,
/// when tx is packed into a new block or dropped by txpool we get a sample about how long a tx with X feerate can get confirmed or get dropped.
///
/// In inner, we group samples by predefined feerate ranges and store them into buckets.
/// To estimator feerate, we travel through these buckets, and try find a proper feerate X to let a tx get confirm with Y propolity within T blocks.
///
pub struct Estimator {
    best_height: u64,
    first_record_height: u64,
    tx_confirm_stat: TxConfirmStat,
    tracked_txs: FnvHashMap<H256, TxRecord>,
}

impl Estimator {
    pub fn new() -> Self {
        let mut buckets = Vec::new();
        let mut bucket_fee_boundary = MIN_BUCKET_FEERATE;
        assert!(MIN_BUCKET_FEERATE > 0f64);
        assert!(FEE_SPACING > 1f64);
        while bucket_fee_boundary <= MAX_BUCKET_FEERATE {
            buckets.push(FeeRate::from_f64(bucket_fee_boundary).expect("feerate"));
            bucket_fee_boundary *= FEE_SPACING;
        }
        Estimator {
            best_height: 0,
            first_record_height: 0,
            tx_confirm_stat: TxConfirmStat::new(&buckets, MAX_CONFIRM_BLOCKS, DEFAULT_DECAY_FACTOR),
            tracked_txs: Default::default(),
        }
    }

    fn process_block_tx(&mut self, height: u64, tx: &TxEntry) -> bool {
        if !self.drop_tx_inner(tx.hash(), false) {
            // tx was not being tracked
            return false;
        }

        let blocks_to_confirm = height.saturating_sub(tx.height) as usize;
        self.tx_confirm_stat
            .add_confirmed_tx(blocks_to_confirm, tx.feerate());
        true
    }

    /// process new block
    pub fn process_block(&mut self, height: u64, txs: &[TxEntry]) {
        // For simpfy, we assume chain reorg will not effect tx fee.
        if height <= self.best_height {
            return;
        }
        self.best_height = height;
        self.tx_confirm_stat.move_track_window(height);
        self.tx_confirm_stat.decay();
        let processed_txs = txs
            .iter()
            .filter(|tx| self.process_block_tx(height, tx))
            .count();
        if self.first_record_height == 0 && processed_txs > 0 {
            // start record
            self.first_record_height = self.best_height;
        }
    }
    /// new enter pool tx
    pub fn track_tx(&mut self, tx: &TxEntry) {
        if self.tracked_txs.contains_key(tx.hash()) {
            // already in track
            return;
        }
        if tx.height != self.best_height {
            // ignore wrong height txs
            return;
        }
        if let Some(bucket_index) = self
            .tx_confirm_stat
            .add_unconfirmed_tx(tx.height, tx.feerate())
        {
            self.tracked_txs.insert(
                tx.hash().to_owned(),
                TxRecord {
                    height: tx.height,
                    bucket_index,
                },
            );
        }
    }

    fn drop_tx_inner(&mut self, tx_hash: &H256, count_failure: bool) -> bool {
        if let Some(tx_entry) = self.tracked_txs.remove(tx_hash) {
            self.tx_confirm_stat.remove_unconfirmed_tx(
                tx_entry.height,
                self.best_height,
                tx_entry.bucket_index,
                count_failure,
            );
            true
        } else {
            false
        }
    }
    /// tx removed by txpool
    pub fn drop_tx(&mut self, tx_hash: &H256) -> bool {
        self.drop_tx_inner(tx_hash, true)
    }

    pub fn estimate(&self, confirm_target: usize) -> FeeRate {
        self.tx_confirm_stat.estimate_median(
            confirm_target,
            MIN_ESTIMATE_SAMPLES,
            MIN_ESTIMATE_CONFIRM_RATE,
        )
    }
}
