use ckb_types::core::Capacity;
use serde_derive::{Deserialize, Serialize};

/// shannons per kilobytes
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeeRate(u64);

const KB: u64 = 1000;

impl FeeRate {
    pub fn calculate(fee: Capacity, vbytes: usize) -> Self {
        if vbytes == 0 {
            return FeeRate::zero();
        }
        FeeRate::from_u64(fee.as_u64().saturating_mul(KB) / (vbytes as u64))
    }

    pub const fn from_u64(fee_per_kb: u64) -> Self {
        FeeRate(fee_per_kb)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn zero() -> Self {
        Self::from_u64(0)
    }

    pub fn fee(self, size: usize) -> Capacity {
        let fee = self.0.saturating_mul(size as u64) / KB;
        Capacity::shannons(fee)
    }
}

impl ::std::fmt::Display for FeeRate {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
