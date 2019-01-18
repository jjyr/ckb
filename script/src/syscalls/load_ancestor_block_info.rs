use crate::syscalls::{ITEM_MISSING, LOAD_ANCESTOR_BLOCK_INFO_SYSCALL_NUMBER, SUCCESS};
use ckb_core::header::{BlockNumber, Header};
use ckb_protocol::Header as FbsHeader;
use ckb_shared::shared::ChainProvider;
use ckb_vm::{CoreMachine, Error as VMError, Memory, Register, Syscalls, A0, A1, A2, A3, A7};
use flatbuffers::FlatBufferBuilder;
use numext_fixed_hash::H256;
use std::cmp;

const BASE_CYCLES: u64 = 100;
const PER_BYTE_CYCLES: u64 = 100;

#[derive(Debug)]
pub struct LoadAncestorBlockInfo<'a, CP: ChainProvider + Clone> {
    provider: &'a CP,
    parent_block_hash: &'a H256,
    parent_block_number: BlockNumber,
}

impl<'a, CP: ChainProvider + Clone> LoadAncestorBlockInfo<'a, CP> {
    pub fn new(
        provider: &'a CP,
        parent_block_hash: &'a H256,
        parent_block_number: BlockNumber,
    ) -> LoadAncestorBlockInfo<'a, CP> {
        LoadAncestorBlockInfo {
            provider,
            parent_block_hash,
            parent_block_number,
        }
    }

    /// Return ancestor block info, return parent if ancestor_number is 0
    fn load_ancestor_block_info(&self, ancestor_number: u8) -> Option<Header> {
        self.parent_block_number
            .checked_sub(u64::from(ancestor_number))
            .and_then(|block_number| {
                self.provider
                    .get_ancestor(&self.parent_block_hash, block_number)
            })
    }
}

impl<'a, R: Register, M: Memory, CP: ChainProvider + Clone> Syscalls<R, M>
    for LoadAncestorBlockInfo<'a, CP>
{
    fn initialize(&mut self, _machine: &mut CoreMachine<R, M>) -> Result<(), VMError> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut CoreMachine<R, M>) -> Result<bool, VMError> {
        let code = machine.registers()[A7].to_u64();
        if code != LOAD_ANCESTOR_BLOCK_INFO_SYSCALL_NUMBER {
            return Ok(false);
        }
        machine.add_cycles(BASE_CYCLES);

        let addr = machine.registers()[A0].to_usize();
        let size_addr = machine.registers()[A1].to_usize();
        let size = machine.memory_mut().load64(size_addr)? as usize;

        let ancestor_number = machine.registers()[A3].to_u8();

        // only support load blocks which block number within std::u8::MAX
        let header = match self.load_ancestor_block_info(ancestor_number) {
            Some(header) => header,
            None => {
                machine.registers_mut()[A0] = R::from_u8(ITEM_MISSING);
                return Ok(true);
            }
        };

        // TODO: find a way to cache this without consuming too much memory
        let mut builder = FlatBufferBuilder::new();
        let offset = FbsHeader::build(&mut builder, &header);
        builder.finish(offset, None);
        let data = builder.finished_data();

        let offset = machine.registers()[A2].to_usize();
        let full_size = data.len() - offset;
        let real_size = cmp::min(size, full_size);
        machine.memory_mut().store64(size_addr, full_size as u64)?;
        machine
            .memory_mut()
            .store_bytes(addr, &data[offset..offset + real_size])?;
        machine.registers_mut()[A0] = R::from_u8(SUCCESS);
        machine.add_cycles(data.len() as u64 * PER_BYTE_CYCLES);
        Ok(true)
    }
}