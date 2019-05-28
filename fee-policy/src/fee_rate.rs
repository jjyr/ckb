use std::cmp::Ordering;

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Default)]
pub struct FeeRate(f64);

impl FeeRate {
    pub fn from_f64(value: f64) -> Option<FeeRate> {
        if value.is_nan() {
            None
        } else {
            Some(FeeRate(value))
        }
    }

    pub fn zero() -> FeeRate {
        FeeRate(0f64)
    }

    pub fn add(&self, other: FeeRate) -> Option<FeeRate> {
        Self::from_f64(self.0 + other.0)
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl Eq for FeeRate {}

impl Ord for FeeRate {
    fn cmp(&self, other: &FeeRate) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
