#![no_std]

extern crate alloc;

use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::cycle::Cycle;

mod component;
mod cycle;
mod decoder;
mod handle_phi2;
mod instruction;
pub mod variant;

pub const RESET_VECTOR: u16 = 0xfffc;
pub const IRQ_VECTOR: u16 = 0xfffe;
pub const NMI_VECTOR: u16 = 0xfffa;
pub const PAGE_SIZE: usize = 256;
pub const STACK_BASE_ADDRESS: u16 = 0x0100;

pub use component::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Bus {
    pub address: u16,
    pub data: u8,
}

/// We don't store this in memory bitpacked for performance reasons
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
struct FlagRegister {
    pub negative: bool,
    pub overflow: bool,
    pub decimal: bool,
    pub interrupt_disable: bool,
    pub zero: bool,
    pub carry: bool,
}

impl FlagRegister {
    pub fn to_byte(self, break_: bool) -> u8 {
        (self.negative as u8) << 7
            | (self.overflow as u8) << 6
            | 1 << 5
            | (break_ as u8) << 4
            | (self.decimal as u8) << 3
            | (self.interrupt_disable as u8) << 2
            | (self.zero as u8) << 1
            | (self.carry as u8)
    }

    pub fn from_byte(byte: u8) -> Self {
        Self {
            negative: (byte >> 7) & 0b0000_0001 != 0,
            overflow: (byte >> 6) & 0b0000_0001 != 0,
            decimal: (byte >> 3) & 0b0000_0001 != 0,
            interrupt_disable: (byte >> 2) & 0b0000_0001 != 0,
            zero: (byte >> 1) & 0b0000_0001 != 0,
            carry: byte & 1 != 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub flags: FlagRegister,
    pub stack: u8,
    pub instruction_pointer: u16,
    pub cycle_queue: heapless::Deque<Cycle, 8>,
    pub bus: Bus,
    pub effective_address: heapless::Vec<u8, 2>,
    pub consume_effective_address: bool,
    pub operand: u8,
    pub rdy: bool,
    pub nmi: NmiFlag,
    pub irq: bool,
}

/// NMI is falling edge
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct NmiFlag {
    current_state: bool,
    falling_edge_occurred: bool,
}

impl Default for NmiFlag {
    fn default() -> Self {
        Self {
            current_state: true,
            falling_edge_occurred: false,
        }
    }
}

impl NmiFlag {
    pub fn store(&mut self, nmi: bool) {
        if core::mem::replace(&mut self.current_state, nmi) && !nmi {
            self.falling_edge_occurred = true;
        }
    }

    pub fn interrupt_required(&mut self) -> bool {
        core::mem::take(&mut self.falling_edge_occurred)
    }
}

#[derive(Debug, Clone)]
pub enum Pin {
    Nmi,
    Irq,
    Rdy,
}

#[derive(Debug, Clone)]
pub enum Mos6502Event {
    FlagChange { pin: Pin, value: bool },
}
