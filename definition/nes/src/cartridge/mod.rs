use bytes::Bytes;
use fluxemu_runtime::memory::AddressSpaceId;

pub mod ines;
pub mod mapper;

#[derive(Debug)]
pub struct CartConfig {
    pub cpu_address_space: AddressSpaceId,
    pub ppu_address_space: AddressSpaceId,
    pub chr: Bytes,
    pub prg: Bytes,
}
