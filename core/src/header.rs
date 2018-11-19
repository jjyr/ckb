use bigint::{H256, U256};
use bincode::serialize;
use hash::sha3_256;
use merkle_root::merkle_root;
use std::ops::{Deref, DerefMut};
use transaction::{IndexedTransaction, ProposalShortId};

const VERSION: u32 = 0;

pub use BlockNumber;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct Seal {
    pub nonce: u64,
    pub proof: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct RawHeader {
    pub version: u32,
    /// Parent hash.
    pub parent_hash: H256,
    /// Block timestamp(ms).
    pub timestamp: u64,
    /// Genesis number is 0, Child block number is parent block number + 1.
    pub number: BlockNumber,
    /// Transactions merkle root.
    pub txs_commit: H256,
    /// Transactions proposal merkle root.
    pub txs_proposal: H256,
    /// Block difficulty.
    pub difficulty: U256,
    /// Hash of the cellbase
    pub cellbase_id: H256,
    /// Hash of the uncles
    pub uncles_hash: H256,
}

impl RawHeader {
    pub fn new<'a>(
        parent_header: &Header,
        commit_transactions: impl Iterator<Item = &'a IndexedTransaction>,
        proposal_short_ids: impl Iterator<Item = &'a ProposalShortId>,
        timestamp: u64,
        difficulty: U256,
        cellbase_id: H256,
        uncles_hash: H256,
    ) -> RawHeader {
        let commit_txs_hash: Vec<H256> = commit_transactions
            .map(|t: &IndexedTransaction| t.hash())
            .collect();
        let txs_commit = merkle_root(commit_txs_hash.as_slice());

        let proposal_txs_hash: Vec<H256> = proposal_short_ids.map(|t| t.hash()).collect();

        let txs_proposal = merkle_root(proposal_txs_hash.as_slice());

        let parent_hash = parent_header.hash();
        let number = parent_header.number + 1;

        RawHeader {
            version: VERSION,
            parent_hash,
            txs_commit,
            txs_proposal,
            timestamp,
            number,
            difficulty,
            cellbase_id,
            uncles_hash,
        }
    }

    pub fn pow_hash(&self) -> H256 {
        sha3_256(serialize(self).unwrap()).into()
    }

    pub fn with_seal(self, seal: Seal) -> Header {
        Header { raw: self, seal }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub raw: RawHeader,
    /// proof seal
    pub seal: Seal,
}

impl Header {
    pub fn hash(&self) -> H256 {
        sha3_256(serialize(self).unwrap()).into()
    }

    pub fn is_genesis(&self) -> bool {
        self.number == 0
    }
}

impl DerefMut for Header {
    fn deref_mut(&mut self) -> &mut RawHeader {
        &mut self.raw
    }
}

impl Deref for Header {
    type Target = RawHeader;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl DerefMut for IndexedHeader {
    fn deref_mut(&mut self) -> &mut Header {
        &mut self.header
    }
}

impl Deref for IndexedHeader {
    type Target = Header;

    fn deref(&self) -> &Self::Target {
        &self.header
    }
}

impl ::std::hash::Hash for IndexedHeader {
    fn hash<H>(&self, state: &mut H)
    where
        H: ::std::hash::Hasher,
    {
        state.write(&self.hash);
        state.finish();
    }
}

#[derive(Clone, Debug, Eq, Default)]
pub struct IndexedHeader {
    pub header: Header,
    /// memorise hash
    hash: H256,
}

impl PartialEq for IndexedHeader {
    fn eq(&self, other: &IndexedHeader) -> bool {
        self.hash == other.hash
    }
}

impl IndexedHeader {
    pub fn hash(&self) -> H256 {
        self.hash
    }

    pub fn new(header: Header, hash: H256) -> Self {
        IndexedHeader { header, hash }
    }

    pub fn finalize_dirty(&mut self) {
        self.hash = self.header.hash();
    }
}

impl From<Header> for IndexedHeader {
    fn from(header: Header) -> Self {
        let hash = header.hash();
        IndexedHeader { header, hash }
    }
}

impl From<IndexedHeader> for Header {
    fn from(indexed_header: IndexedHeader) -> Self {
        indexed_header.header
    }
}