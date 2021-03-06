//! Generated packed bytes wrappers.

#![allow(clippy::all)]
#![allow(unused_imports)]

mod blockchain;
mod extensions;

pub mod packed {
    pub use molecule::prelude::{Byte, ByteReader};

    pub use super::blockchain::*;
    pub use super::extensions::*;
}
