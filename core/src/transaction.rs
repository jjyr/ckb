//! Transaction using Cell.
//! It is similar to Bitcoin Tx <https://en.bitcoin.it/wiki/Protocol_documentation#tx/>
use crate::script::Script;
use crate::{BlockNumber, Bytes, Version};
pub use bincode::deserialize;
use canonical_serializer::{CanonicalSerialize, CanonicalSerializer, Result as SerializeResult};
use ckb_util::LowerHexOption;
use faster_hex::hex_string;
use hash::{blake2b_256, Blake2bWriter};
use numext_fixed_hash::{h256, H256};
use occupied_capacity::{Capacity, Result as CapacityResult};
use serde_derive::{Deserialize, Serialize};
use std::convert::TryInto;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::mem;
use std::ops::{Deref, DerefMut};

pub const TX_VERSION: Version = 0;

// This is the tx hash used to mark the input is actually a special input for
// issuing DAO interests. When this is encountered, CKB will skip resolving and
// script validation. All CKB does is to validate that the transaction has
// another input referencing a DAO cell(determined by having DAO_CODE_HASH as
// code hash of lock script). The actual DAO validation logic is left to the
// unlocking process of the DAO cell. The hex used here is actually
// "NERVOSDAOINPUT0001" in hex mode.
pub const ISSUING_DAO_HASH: H256 = h256!("0x4e4552564f5344414f494e50555430303031");

pub struct CellKey([u8; 36]);

impl CellKey {
    pub fn calculate(tx_hash: &H256, index: u32) -> Self {
        let mut key: [u8; 36] = [0; 36];
        key[..32].copy_from_slice(tx_hash.as_bytes());
        key[32..36].copy_from_slice(&index.to_le_bytes());
        CellKey(key)
    }

    pub fn recover(&self) -> CellOutPoint {
        Self::deconstruct(&self.0)
    }

    pub fn deconstruct(bytes: &[u8]) -> CellOutPoint {
        let tx_hash = H256::from_slice(&bytes[..32]).expect("should not be failed");
        let le_bytes: [u8; 4] = bytes[32..36].try_into().expect("should not be failed");
        let index = u32::from_le_bytes(le_bytes);
        CellOutPoint { tx_hash, index }
    }
}

impl AsRef<[u8]> for CellKey {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

#[derive(Eq, PartialEq)]
pub struct CellOutPointRef<'a> {
    tx_hash: &'a H256,
    index: u32,
}

impl<'a> CellOutPointRef<'a> {
    pub fn new(tx_hash: &'a H256, index: u32) -> CellOutPointRef<'a> {
        CellOutPointRef { tx_hash, index }
    }

    pub fn cell_key(&self) -> CellKey {
        CellKey::calculate(self.tx_hash, self.index)
    }
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct CellOutPoint {
    // Hash of Transaction
    pub tx_hash: H256,
    // Index of output
    pub index: u32,
}

impl CanonicalSerialize for CellOutPoint {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer
            .encode_struct_ref(&self.tx_hash)?
            .encode_u32(self.index)?;
        Ok(())
    }
}

impl fmt::Debug for CellOutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CellOutPoint")
            .field("tx_hash", &format_args!("{:#x}", self.tx_hash))
            .field("index", &self.index)
            .finish()
    }
}

impl Default for CellOutPoint {
    fn default() -> Self {
        CellOutPoint {
            tx_hash: H256::zero(),
            index: u32::max_value(),
        }
    }
}

impl CellOutPoint {
    pub fn destruct(self) -> (H256, u32) {
        let CellOutPoint { tx_hash, index } = self;
        (tx_hash, index)
    }

    pub const fn serialized_size() -> usize {
        H256::size_of() + mem::size_of::<u32>()
    }

    pub fn cell_key(&self) -> CellKey {
        CellKey::calculate(&self.tx_hash, self.index)
    }

    pub fn to_ref(&self) -> CellOutPointRef {
        CellOutPointRef {
            tx_hash: &self.tx_hash,
            index: self.index,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct OutPoint {
    pub cell: Option<CellOutPoint>,
    pub block_hash: Option<H256>,
}
impl CanonicalSerialize for OutPoint {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer
            .encode_option_ref(&self.cell)?
            .encode_option_ref(&self.block_hash)?;
        Ok(())
    }
}

impl fmt::Debug for OutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("OutPoint")
            .field("cell", &self.cell)
            .field(
                "block_hash",
                &format_args!("{:#x}", LowerHexOption(self.block_hash.as_ref())),
            )
            .finish()
    }
}

impl OutPoint {
    pub fn new(block_hash: H256, tx_hash: H256, index: u32) -> Self {
        OutPoint {
            block_hash: Some(block_hash),
            cell: Some(CellOutPoint { tx_hash, index }),
        }
    }

    pub fn new_cell(tx_hash: H256, index: u32) -> Self {
        OutPoint {
            block_hash: None,
            cell: Some(CellOutPoint { tx_hash, index }),
        }
    }

    pub fn new_block_hash(block_hash: H256) -> Self {
        OutPoint {
            block_hash: Some(block_hash),
            cell: None,
        }
    }

    pub fn new_issuing_dao() -> Self {
        OutPoint::new_cell(ISSUING_DAO_HASH, 0)
    }

    pub fn null() -> Self {
        OutPoint::default()
    }

    pub fn is_null(&self) -> bool {
        self.cell.is_none() && self.block_hash.is_none()
    }

    pub fn serialized_size(&self) -> usize {
        self.cell
            .as_ref()
            .map(|_| CellOutPoint::serialized_size())
            .unwrap_or(0)
            + self
                .block_hash
                .as_ref()
                .map(|_| H256::size_of())
                .unwrap_or(0)
    }

    pub fn is_issuing_dao(&self) -> bool {
        self.block_hash.is_none()
            && self
                .cell
                .as_ref()
                .map(|cell| cell.tx_hash == ISSUING_DAO_HASH && cell.index == 0)
                .unwrap_or(false)
    }

    pub fn destruct(self) -> (Option<H256>, Option<CellOutPoint>) {
        let OutPoint { block_hash, cell } = self;
        (block_hash, cell)
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct CellInput {
    pub previous_output: OutPoint,
    pub since: u64,
}

impl CanonicalSerialize for CellInput {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        self.previous_output.serialize(serializer)?;
        serializer.encode_u64(self.since)?;
        Ok(())
    }
}

impl CellInput {
    pub fn new(previous_output: OutPoint, since: u64) -> Self {
        CellInput {
            previous_output,
            since,
        }
    }

    pub fn new_cellbase_input(block_number: BlockNumber) -> Self {
        CellInput {
            previous_output: OutPoint::null(),
            since: block_number,
        }
    }

    pub fn destruct(self) -> (OutPoint, u64) {
        let CellInput {
            previous_output,
            since,
        } = self;
        (previous_output, since)
    }

    pub fn serialized_size(&self) -> usize {
        self.previous_output.serialized_size() + mem::size_of::<u64>()
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CellOutput {
    pub capacity: Capacity,
    pub data: Bytes,
    pub lock: Script,
    #[serde(rename = "type")]
    pub type_: Option<Script>,
}

impl CanonicalSerialize for CellOutput {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer
            .encode_u64(self.capacity.as_u64())?
            .encode_struct_ref(&self.lock)?
            .encode_option_ref(&self.type_)?
            .encode_bytes(&self.data)?;
        Ok(())
    }
}

impl fmt::Debug for CellOutput {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CellOutput")
            .field("capacity", &self.capacity)
            .field(
                "data",
                &format_args!("0x{}", &hex_string(&self.data).expect("hex data")),
            )
            .field("lock", &self.lock)
            .field("type", &self.type_)
            .finish()
    }
}

impl CellOutput {
    pub fn new(capacity: Capacity, data: Bytes, lock: Script, type_: Option<Script>) -> Self {
        CellOutput {
            capacity,
            data,
            lock,
            type_,
        }
    }

    pub fn data_hash(&self) -> H256 {
        blake2b_256(&self.data).into()
    }

    pub fn serialized_size(&self) -> usize {
        mem::size_of::<u64>()
            + self.data.len()
            + 4
            + self.lock.serialized_size()
            + self
                .type_
                .as_ref()
                .map(Script::serialized_size)
                .unwrap_or(0)
    }

    pub fn destruct(self) -> (Capacity, Bytes, Script, Option<Script>) {
        let CellOutput {
            capacity,
            data,
            lock,
            type_,
        } = self;
        (capacity, data, lock, type_)
    }

    pub fn occupied_capacity(&self) -> CapacityResult<Capacity> {
        Capacity::bytes(8 + self.data.len())
            .and_then(|x| self.lock.occupied_capacity().and_then(|y| y.safe_add(x)))
            .and_then(|x| {
                self.type_
                    .as_ref()
                    .map(Script::occupied_capacity)
                    .transpose()
                    .and_then(|y| y.unwrap_or_else(Capacity::zero).safe_add(x))
            })
    }

    pub fn is_lack_of_capacity(&self) -> CapacityResult<bool> {
        self.occupied_capacity().map(|cap| cap > self.capacity)
    }
}

pub type Witness = Vec<Bytes>;

#[derive(Clone, Serialize, Eq, Debug)]
pub struct Transaction {
    version: Version,
    deps: Vec<OutPoint>,
    inputs: Vec<CellInput>,
    outputs: Vec<CellOutput>,
    //Segregated Witness to provide protection from transaction malleability.
    witnesses: Vec<Witness>,
    #[serde(skip)]
    hash: H256,
    #[serde(skip)]
    witness_hash: H256,
}

impl<'a> CanonicalSerialize for Transaction {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer
            .encode_u32(self.version)?
            .encode_vec(&self.deps)?
            .encode_vec(&self.inputs)?
            .encode_vec(&self.outputs)?
            .encode_vec(&self.witnesses)?;
        Ok(())
    }
}

impl<'de> serde::de::Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Version,
            Deps,
            Inputs,
            Outputs,
            Witnesses,
        }

        struct InnerVisitor;

        impl<'de> serde::de::Visitor<'de> for InnerVisitor {
            type Value = Transaction;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Transaction")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
            where
                V: serde::de::SeqAccess<'de>,
            {
                let version = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let deps = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let inputs = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let outputs = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                let witnesses = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                Ok(Self::Value::new(version, deps, inputs, outputs, witnesses))
            }

            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                let mut version = None;
                let mut deps = None;
                let mut inputs = None;
                let mut outputs = None;
                let mut witnesses = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Version => {
                            if version.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        }
                        Field::Deps => {
                            if deps.is_some() {
                                return Err(serde::de::Error::duplicate_field("deps"));
                            }
                            deps = Some(map.next_value()?);
                        }
                        Field::Inputs => {
                            if inputs.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputs"));
                            }
                            inputs = Some(map.next_value()?);
                        }
                        Field::Outputs => {
                            if outputs.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputs"));
                            }
                            outputs = Some(map.next_value()?);
                        }
                        Field::Witnesses => {
                            if witnesses.is_some() {
                                return Err(serde::de::Error::duplicate_field("witnesses"));
                            }
                            witnesses = Some(map.next_value()?);
                        }
                    }
                }
                let version = version.ok_or_else(|| serde::de::Error::missing_field("version"))?;
                let deps = deps.ok_or_else(|| serde::de::Error::missing_field("deps"))?;
                let inputs = inputs.ok_or_else(|| serde::de::Error::missing_field("inputs"))?;
                let outputs = outputs.ok_or_else(|| serde::de::Error::missing_field("outputs"))?;
                let witnesses =
                    witnesses.ok_or_else(|| serde::de::Error::missing_field("witnesses"))?;
                Ok(Self::Value::new(version, deps, inputs, outputs, witnesses))
            }
        }

        const FIELDS: &[&str] = &["version", "deps", "inputs", "outputs", "witnesses"];
        deserializer.deserialize_struct("Transaction", FIELDS, InnerVisitor)
    }
}

#[derive(Serialize)]
struct RawTransaction<'a> {
    version: Version,
    deps: &'a [OutPoint],
    inputs: &'a [CellInput],
    outputs: &'a [CellOutput],
}

impl<'a> CanonicalSerialize for RawTransaction<'a> {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer
            .encode_u32(self.version)?
            .encode_vec(self.deps)?
            .encode_vec(self.inputs)?
            .encode_vec(self.outputs)?;
        Ok(())
    }
}

impl<'a> RawTransaction<'a> {
    fn compute_hash(&self) -> H256 {
        let mut hasher = Blake2bWriter::new();
        let mut serializer = CanonicalSerializer::new(&mut hasher);
        self.serialize(&mut serializer)
            .expect("RawTransaction canonical serialize");
        hasher.finalize().into()
    }
}

impl Hash for Transaction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.witness_hash().as_fixed_bytes())
    }
}

impl PartialEq for Transaction {
    fn eq(&self, other: &Transaction) -> bool {
        self.witness_hash() == other.witness_hash()
    }
}

impl Transaction {
    pub(crate) fn new(
        version: Version,
        deps: Vec<OutPoint>,
        inputs: Vec<CellInput>,
        outputs: Vec<CellOutput>,
        witnesses: Vec<Witness>,
    ) -> Self {
        let raw = RawTransaction {
            version,
            deps: &deps,
            inputs: &inputs,
            outputs: &outputs,
        };
        let hash = raw.compute_hash();
        let mut tx = Self {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
            hash,
            witness_hash: H256::zero(),
        };
        tx.witness_hash = tx.compute_witness_hash();
        tx
    }

    fn compute_witness_hash(&self) -> H256 {
        let mut hasher = Blake2bWriter::new();
        let mut serializer = CanonicalSerializer::new(&mut hasher);
        self.serialize(&mut serializer)
            .expect("Transaction canonical serialize");
        hasher.finalize().into()
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn deps(&self) -> &[OutPoint] {
        &self.deps
    }

    pub fn inputs(&self) -> &[CellInput] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[CellOutput] {
        &self.outputs
    }

    pub fn witnesses(&self) -> &[Witness] {
        &self.witnesses
    }

    // one-in one-out one-wit
    pub fn is_cellbase(&self) -> bool {
        self.inputs.len() == 1
            && self.outputs.len() == 1
            && self.witnesses.len() == 1
            && self.inputs[0].previous_output.is_null()
    }

    pub fn is_withdrawing_from_dao(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| input.previous_output.is_issuing_dao())
    }

    pub fn hash(&self) -> &H256 {
        &self.hash
    }

    pub fn witness_hash(&self) -> &H256 {
        &self.witness_hash
    }

    pub fn output_pts(&self) -> Vec<OutPoint> {
        let h = self.hash();
        (0..self.outputs.len())
            .map(|x| OutPoint::new_cell(h.clone(), x as u32))
            .collect()
    }

    pub fn input_pts_iter(&self) -> impl Iterator<Item = &OutPoint> {
        self.inputs.iter().map(|x| &x.previous_output)
    }

    pub fn deps_iter(&self) -> impl Iterator<Item = &OutPoint> {
        self.deps.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty() || self.outputs.is_empty()
    }

    // proposal_short_id
    pub fn proposal_short_id(&self) -> ProposalShortId {
        ProposalShortId::from_tx_hash(&self.hash())
    }

    pub fn get_output(&self, i: usize) -> Option<CellOutput> {
        self.outputs.get(i).cloned()
    }

    pub fn outputs_capacity(&self) -> ::occupied_capacity::Result<Capacity> {
        self.outputs
            .iter()
            .map(|output| output.capacity)
            .try_fold(Capacity::zero(), Capacity::safe_add)
    }

    pub fn serialized_size(&self) -> usize {
        mem::size_of::<Version>()
            + self
                .deps
                .iter()
                .map(OutPoint::serialized_size)
                .sum::<usize>()
            + 4
            + self
                .inputs
                .iter()
                .map(CellInput::serialized_size)
                .sum::<usize>()
            + 4
            + self
                .outputs
                .iter()
                .map(CellOutput::serialized_size)
                .sum::<usize>()
            + 4
            + self
                .witnesses
                .iter()
                .flat_map(|witness| witness.iter().map(Bytes::len))
                .sum::<usize>()
            + 4
    }
}

#[derive(Default)]
pub struct TransactionBuilder {
    version: Version,
    deps: Vec<OutPoint>,
    inputs: Vec<CellInput>,
    outputs: Vec<CellOutput>,
    witnesses: Vec<Witness>,
}

impl TransactionBuilder {
    pub fn new(bytes: &[u8]) -> Self {
        let Transaction {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
            ..
        } = deserialize(bytes).expect("transaction deserializing should be ok");
        Self {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
        }
    }

    pub fn from_transaction(transaction: Transaction) -> Self {
        let Transaction {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
            ..
        } = transaction;
        Self {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
        }
    }

    pub fn version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn dep(mut self, dep: OutPoint) -> Self {
        self.deps.push(dep);
        self
    }

    pub fn deps(mut self, deps: Vec<OutPoint>) -> Self {
        self.deps.extend(deps);
        self
    }

    pub fn deps_clear(mut self) -> Self {
        self.deps.clear();
        self
    }

    pub fn input(mut self, input: CellInput) -> Self {
        self.inputs.push(input);
        self
    }

    pub fn inputs(mut self, inputs: Vec<CellInput>) -> Self {
        self.inputs.extend(inputs);
        self
    }

    pub fn inputs_clear(mut self) -> Self {
        self.inputs.clear();
        self
    }

    pub fn output(mut self, output: CellOutput) -> Self {
        self.outputs.push(output);
        self
    }

    pub fn outputs(mut self, outputs: Vec<CellOutput>) -> Self {
        self.outputs.extend(outputs);
        self
    }

    pub fn outputs_clear(mut self) -> Self {
        self.outputs.clear();
        self
    }

    pub fn witness(mut self, witness: Witness) -> Self {
        self.witnesses.push(witness);
        self
    }

    pub fn witnesses(mut self, witness: Vec<Witness>) -> Self {
        self.witnesses.extend(witness);
        self
    }

    pub fn witnesses_clear(mut self) -> Self {
        self.witnesses.clear();
        self
    }

    pub fn build(self) -> Transaction {
        let Self {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
        } = self;
        Transaction::new(version, deps, inputs, outputs, witnesses)
    }

    /// # Warning
    ///
    /// When using this method, the caller should ensure the input hashes is right, or the caller
    /// will get a incorrect Transaction.
    pub unsafe fn build_unchecked(self, hash: H256, witness_hash: H256) -> Transaction {
        let Self {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
        } = self;
        Transaction {
            version,
            deps,
            inputs,
            outputs,
            witnesses,
            hash,
            witness_hash,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IndexTransaction {
    pub index: usize,
    pub transaction: Transaction,
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Default, Hash)]
pub struct ProposalShortId([u8; 10]);

impl CanonicalSerialize for ProposalShortId {
    fn serialize<W: Write>(&self, serializer: &mut CanonicalSerializer<W>) -> SerializeResult<()> {
        serializer.encode_fix_length_bytes(&self.0)?;
        Ok(())
    }
}

impl Deref for ProposalShortId {
    type Target = [u8; 10];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for ProposalShortId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ProposalShortId(0x{})",
            hex_string(&self.0).expect("hex proposal short id")
        )
    }
}

impl DerefMut for ProposalShortId {
    fn deref_mut(&mut self) -> &mut [u8; 10] {
        &mut self.0
    }
}

impl ProposalShortId {
    pub fn new(inner: [u8; 10]) -> Self {
        ProposalShortId(inner)
    }

    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() == 10usize {
            let mut id = [0u8; 10];
            id.copy_from_slice(slice);
            Some(ProposalShortId(id))
        } else {
            None
        }
    }

    pub fn from_tx_hash(h: &H256) -> Self {
        let mut inner = [0u8; 10];
        inner.copy_from_slice(&h.as_bytes()[..10]);
        ProposalShortId(inner)
    }

    pub fn zero() -> Self {
        ProposalShortId([0; 10])
    }

    pub fn into_inner(self) -> [u8; 10] {
        self.0
    }

    pub const fn serialized_size() -> usize {
        mem::size_of::<[u8; 10]>()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{capacity_bytes, Bytes, Capacity};

    #[test]
    fn tx_hash() {
        let tx = TransactionBuilder::default()
            .output(CellOutput::new(
                capacity_bytes!(5000),
                Bytes::from(vec![1, 2, 3]),
                Script::default(),
                None,
            ))
            .input(CellInput::new(OutPoint::new_cell(H256::zero(), 0), 0))
            .witness(vec![Bytes::from(vec![7, 8, 9])])
            .build();

        assert_eq!(
            format!("{:x}", tx.hash()),
            "b8af0234bf75c516b190e08fc4d9f154cc44875c6f7c0d84aef802b171346d06"
        );
        assert_eq!(
            format!("{:x}", tx.witness_hash()),
            "93714ca8acad869e53709f7f57d3249bc4d89d0d5c6f936a9cc57214326d49c5"
        );
    }

    #[test]
    fn min_cell_output_capacity() {
        let lock = Script::new(vec![], H256::default());
        let output = CellOutput::new(Capacity::zero(), Default::default(), lock, None);
        assert_eq!(output.occupied_capacity().unwrap(), capacity_bytes!(40));
    }

    #[test]
    fn min_secp256k1_cell_output_capacity() {
        let lock = Script::new(vec![vec![0u8; 20].into()], H256::default());
        let output = CellOutput::new(Capacity::zero(), Default::default(), lock, None);
        assert_eq!(output.occupied_capacity().unwrap(), capacity_bytes!(60));
    }
}
