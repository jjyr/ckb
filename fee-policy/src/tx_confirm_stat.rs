use crate::fee_rate::FeeRate;
use log::debug;
use std::collections::BTreeMap;

#[derive(Default)]
struct BucketStat {
    total_feerate: FeeRate,
    txs_count: f64,
    old_unconfirmed_txs: usize,
}

impl BucketStat {
    fn inc_total_feerate(&mut self, feerate: FeeRate) {
        match self.total_feerate.add(feerate) {
            Some(total) => self.total_feerate = total,
            None => debug!(
                "inc_total_feerate NaN, total_feerate: {:?}, inc_feerate: {:?}",
                self.total_feerate, feerate
            ),
        }
    }

    fn avg_feerate(&self) -> Option<FeeRate> {
        if self.txs_count != 0f64 {
            FeeRate::from_f64(self.total_feerate.value() / self.txs_count as f64)
        } else {
            None
        }
    }
}

/// Track tx feerate and confirmation time,
/// this struct track unconfirmed txs count when tx added to or remove from txpool,
/// when a tx confirmed, put it into buckets by tx feerate,
/// estimate median fee by look up each buckets until meet confirm_rate.
///
/// TODO support decay to reduce stat values.
/// TODO add tests
/// TODO impl upper level abstract
pub struct TxConfirmStat {
    /// per bucket stat
    bucket_stats: Vec<BucketStat>,
    /// bucket upper bound feerate => bucket index
    feerate_to_bucket: BTreeMap<FeeRate, usize>,
    /// confirm_target => bucket index => confirmed txs count
    confirm_target_to_confirmed_txs: Vec<Vec<f64>>,
    /// confirm_target => bucket index => failed txs count
    confirm_target_to_failed_txs: Vec<Vec<f64>>,
    /// Track recent N blocks unconfirmed txs
    /// tracked block index => bucket index => TxTracker
    block_unconfirmed_txs: Vec<Vec<usize>>,
    decay_factor: f64,
}

impl TxConfirmStat {
    pub fn new(buckets: &[FeeRate], max_confirm_blocks: usize, decay_factor: f64) -> Self {
        let mut bucket_stats = Vec::with_capacity(buckets.len());
        bucket_stats.resize_with(buckets.len(), BucketStat::default);
        let feerate_to_bucket = buckets
            .iter()
            .enumerate()
            .map(|(i, feerate)| (*feerate, i))
            .collect();
        let mut confirm_target_to_confirmed_txs = Vec::with_capacity(max_confirm_blocks);
        confirm_target_to_confirmed_txs.resize_with(max_confirm_blocks, Vec::new);
        confirm_target_to_confirmed_txs
            .iter_mut()
            .for_each(|bucket| {
                bucket.resize(buckets.len(), 0f64);
            });
        let mut confirm_target_to_failed_txs = Vec::with_capacity(max_confirm_blocks);
        confirm_target_to_failed_txs.resize_with(max_confirm_blocks, Vec::new);
        confirm_target_to_failed_txs.iter_mut().for_each(|bucket| {
            bucket.resize(buckets.len(), 0f64);
        });
        let mut block_unconfirmed_txs = Vec::with_capacity(max_confirm_blocks);
        block_unconfirmed_txs.resize_with(max_confirm_blocks, Vec::new);
        block_unconfirmed_txs.iter_mut().for_each(|bucket| {
            bucket.resize(buckets.len(), 0);
        });
        TxConfirmStat {
            bucket_stats,
            feerate_to_bucket,
            block_unconfirmed_txs,
            confirm_target_to_confirmed_txs,
            confirm_target_to_failed_txs,
            decay_factor,
        }
    }

    /// Return upper bound feerate bucket
    /// assume we have three buckets with feerate [1.0, 2.0, 3.0], we return index 1 for feerate 1.5
    fn bucket_index_by_feerate(&self, feerate: FeeRate) -> Option<usize> {
        self.feerate_to_bucket
            .range(feerate..)
            .next()
            .map(|(_feerate, index)| *index)
    }

    fn max_confirms(&self) -> usize {
        self.confirm_target_to_confirmed_txs.len()
    }

    // add confirmed sample
    pub fn add_confirmed_tx(&mut self, blocks_to_confirm: usize, feerate: FeeRate) {
        if blocks_to_confirm < 1 {
            return;
        }
        let bucket_index = match self.bucket_index_by_feerate(feerate) {
            Some(index) => index,
            None => return,
        };
        // increase txs_count in buckets
        for i in (blocks_to_confirm - 1)..self.max_confirms() {
            self.confirm_target_to_confirmed_txs[i][bucket_index] += 1f64;
        }
        let mut stat = &mut self.bucket_stats[bucket_index];
        stat.txs_count += 1f64;
        stat.inc_total_feerate(feerate);
    }

    // track an unconfirmed tx
    // entry_height - tip number when tx enter txpool
    pub fn add_unconfirmed_tx(&mut self, entry_height: u64, feerate: FeeRate) -> Option<usize> {
        let bucket_index = match self.bucket_index_by_feerate(feerate) {
            Some(index) => index,
            None => return None,
        };
        let block_index = (entry_height % (self.block_unconfirmed_txs.len() as u64)) as usize;
        self.block_unconfirmed_txs[block_index][bucket_index] += 1;
        Some(bucket_index)
    }

    pub fn remove_unconfirmed_tx(
        &mut self,
        entry_height: u64,
        tip_height: u64,
        bucket_index: usize,
        count_failure: bool,
    ) {
        let tx_age = tip_height.saturating_sub(entry_height) as usize;
        if tx_age < 1 {
            return;
        }
        if tx_age >= self.block_unconfirmed_txs.len() {
            self.bucket_stats[bucket_index].old_unconfirmed_txs -= 1;
        } else {
            let block_index = (entry_height % self.block_unconfirmed_txs.len() as u64) as usize;
            self.block_unconfirmed_txs[block_index][bucket_index] -= 1;
        }
        if count_failure {
            self.confirm_target_to_failed_txs[tx_age - 1][bucket_index] += 1f64;
        }
    }

    pub fn move_track_window(&mut self, height: u64) {
        let block_index = (height % (self.block_unconfirmed_txs.len() as u64)) as usize;
        for bucket_index in 0..self.bucket_stats.len() {
            // mark unconfirmed txs as old_unconfirmed_txs
            self.bucket_stats[bucket_index].old_unconfirmed_txs +=
                self.block_unconfirmed_txs[block_index][bucket_index];
            self.block_unconfirmed_txs[block_index][bucket_index] = 0;
        }
    }

    /// apply decay factor on stats
    /// this behavior will smoothly remove the effects from old data, and moving forward to effects from new data.
    pub fn decay(&mut self) {
        let decay_factor = self.decay_factor;
        for (bucket_index, bucket) in self.bucket_stats.iter_mut().enumerate() {
            self.confirm_target_to_confirmed_txs
                .iter_mut()
                .for_each(|buckets| {
                    buckets[bucket_index] *= decay_factor;
                });

            self.confirm_target_to_failed_txs
                .iter_mut()
                .for_each(|buckets| {
                    buckets[bucket_index] *= decay_factor;
                });
            bucket.total_feerate = FeeRate::from_f64(bucket.total_feerate.value() * decay_factor)
                .expect("decay total feerate");
            bucket.txs_count *= decay_factor;
            // TODO do we need decay the old unconfirmed?
        }
    }

    /// The naive estimate implementation
    /// 1. find best range of buckets satisfy the given condition
    /// 2. get median feerate from best range bucekts
    pub fn estimate_median(
        &self,
        confirm_target: usize,
        required_samples: usize,
        confirm_rate: f64,
    ) -> FeeRate {
        if confirm_target < 1 {
            return FeeRate::zero();
        }
        let mut confirmed_txs = 0f64;
        let mut txs_count = 0f64;
        let mut failure_count = 0f64;
        let mut extra_count = 0;
        let mut best_bucket_start = 0;
        let mut best_bucket_end = 0;
        let start_bucket_index = 0;
        for (bucket_index, stat) in self.bucket_stats.iter().enumerate() {
            confirmed_txs += self.confirm_target_to_confirmed_txs[confirm_target - 1][bucket_index];
            failure_count += self.confirm_target_to_failed_txs[confirm_target - 1][bucket_index];
            extra_count += &self.block_unconfirmed_txs[confirm_target - 1][bucket_index];
            txs_count += stat.txs_count;
            // we have enough data
            if txs_count > required_samples as f64 {
                let confirmed_percent =
                    confirmed_txs / (txs_count + failure_count + extra_count as f64);
                // found best buckets range
                if confirmed_percent > confirm_rate {
                    best_bucket_start = start_bucket_index;
                    best_bucket_end = bucket_index;
                    break;
                } else {
                    // failed, continue try
                    continue;
                }
            }
        }

        let best_range_txs_count: f64 = self.bucket_stats[best_bucket_start..=best_bucket_end]
            .iter()
            .map(|b| b.txs_count)
            .sum();

        // find median bucket
        if best_range_txs_count != 0f64 {
            let mut half_count = best_range_txs_count / 2f64;
            for bucket in &self.bucket_stats[best_bucket_start..=best_bucket_end] {
                // found the median bucket
                if bucket.txs_count >= half_count {
                    return bucket.avg_feerate().unwrap_or_else(|| FeeRate::zero());
                } else {
                    half_count -= bucket.txs_count;
                }
            }
        }
        FeeRate::zero()
    }
}
