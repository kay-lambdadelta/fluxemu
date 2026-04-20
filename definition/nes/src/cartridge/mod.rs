use bytes::Bytes;
use fluxemu_runtime::{ComponentPath, memory::AddressSpaceId};

pub mod ines;
pub mod mapper;

#[derive(Debug)]
pub struct CartParams {
    pub cpu_address_space: AddressSpaceId,
    pub ppu_address_space: AddressSpaceId,
    pub nametables: [ComponentPath; 2],
    pub chr_rom: Option<Bytes>,
    pub prg_rom: Bytes,
    pub prg_ram_size: usize,
    pub chr_ram_size: usize,
    pub chr_nvram_size: usize,
}
