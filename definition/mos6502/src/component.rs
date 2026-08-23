use core::marker::PhantomData;

use alloc::boxed::Box;
use fluxemu_runtime::{
    Platform, RuntimeHandle,
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId},
    scheduler::{
        Frequency, QuantaAllocator,
        event::{Event, downcast_event},
        task::{FrequencyBased, Mode},
    },
};

use crate::{
    Bus, FlagRegister, IRQ_VECTOR, Mos6502Event, NMI_VECTOR, NmiFlag, Pin, RESET_VECTOR,
    STACK_BASE_ADDRESS, State,
    cycle::{BusMode, Cycle, Flag, MoveDestination, MoveSource, Phi1, Phi2, SetAddressBusSource},
    variant::Variant,
};

#[derive(Debug)]
pub struct Mos6502<V: Variant> {
    pub(crate) state: State,
    pub(crate) config: Config<V>,
    _variant: PhantomData<V>,
}

impl<V: Variant> Component for Mos6502<V> {
    type Event = Mos6502Event;

    fn handle_event(&mut self, event: Box<dyn Event>) {
        let event = downcast_event::<Self>(event);

        match event {
            Mos6502Event::FlagChange { pin: flag, value } => match flag {
                Pin::Nmi => self.state.nmi.store(value),
                Pin::Irq => self.state.irq = value,
                Pin::Rdy => self.state.rdy = value,
            },
        }
    }
}

#[derive(Debug)]
pub struct Config<V: Variant> {
    frequency: Frequency,
    assigned_address_space: AddressSpaceId,
    _phantom: PhantomData<V>,
}

impl<V: Variant> Config<V> {
    pub fn new(frequency: Frequency, assigned_address_space: AddressSpaceId) -> Self {
        Self {
            frequency,
            assigned_address_space,
            _phantom: PhantomData,
        }
    }
}

impl<P: Platform, V: Variant> ComponentConfig<P> for Config<V> {
    type Component = Mos6502<V>;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        component_builder.task(
            "synchronization",
            Mode::Always,
            FrequencyBased::new(self.frequency, Self::Component::task),
        );

        let mut component = Mos6502 {
            state: State {
                a: 0,
                x: 0,
                y: 0,
                flags: FlagRegister::default(),
                stack: 0xff,
                // Will be set later
                instruction_pointer: 0x0000,
                cycle_queue: heapless::Deque::default(),
                operand: 0,
                bus: Bus {
                    address: 0x0000,
                    data: 0x00,
                },
                rdy: true,
                irq: false,
                nmi: NmiFlag::default(),
                effective_address: heapless::Vec::default(),
                consume_effective_address: false,
            },
            config: self,
            _variant: PhantomData::<V>,
        };

        // Put it in the reset state for startup
        component.reset();

        Ok(component)
    }
}

impl<V: Variant> Mos6502<V> {
    pub fn address_space(&self) -> AddressSpaceId {
        self.config.assigned_address_space
    }

    fn reset(&mut self) {
        self.state.cycle_queue.clear();
        self.state.cycle_queue.extend([
            // Two dummy cycles
            Cycle::new(BusMode::Read, None, []),
            Cycle::new(BusMode::Read, None, []),
            // Initialize the stack
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::Move {
                    source: MoveSource::Constant(0xfd),
                    destination: MoveDestination::Stack,
                }],
            ),
            // Sets flags
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::Move {
                    source: MoveSource::Constant(
                        FlagRegister {
                            negative: false,
                            overflow: false,
                            decimal: false,
                            interrupt_disable: true,
                            zero: false,
                            carry: false,
                        }
                        .to_byte(false),
                    ),
                    destination: MoveDestination::Flags,
                }],
            ),
            // Load the reset vector
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(RESET_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(RESET_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                ],
            ),
        ]);
    }

    fn handle_nmi(&mut self) {
        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 1 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 0 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Flags { break_: false },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(NMI_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(NMI_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                ],
            ),
        ]);
    }

    fn handle_irq(&mut self) {
        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 1 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 0 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Flags { break_: false },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(IRQ_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(IRQ_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                    Phi2::SetFlag {
                        flag: Flag::InterruptDisable,
                        value: true,
                    },
                ],
            ),
        ]);
    }

    #[inline]
    fn task(&mut self, runtime_handle: &RuntimeHandle, quanta_allocator: QuantaAllocator<'_, '_>) {
        let mut address_space = runtime_handle
            .address_space(self.config.assigned_address_space)
            .unwrap();

        for timestamp in quanta_allocator {
            if self.state.cycle_queue.is_empty() {
                self.state
                    .cycle_queue
                    .push_back(Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
                        [
                            Phi2::IncrementInstructionPointer,
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::Opcode,
                            },
                        ],
                    ))
                    .unwrap();
            }

            let current_cycle = self.state.cycle_queue.front_mut().unwrap();

            match current_cycle.phi1 {
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }) => {
                    self.state.bus.address = self.state.instruction_pointer;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::EffectiveAddress,
                }) => {
                    match self.state.effective_address.len() {
                        1 => {
                            self.state.bus.address = u16::from(self.state.effective_address[0]);
                        }
                        2 => {
                            self.state.bus.address = u16::from_le_bytes([
                                self.state.effective_address[0],
                                self.state.effective_address[1],
                            ]);
                        }
                        _ => unreachable!(),
                    }

                    self.state.consume_effective_address = true;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(value),
                }) => {
                    self.state.bus.address = value;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }) => {
                    self.state.bus.address = u16::from(self.state.stack) | STACK_BASE_ADDRESS;
                }
                None => {}
            }

            let is_read_cycle = match current_cycle.bus_mode {
                BusMode::Read => {
                    self.state.bus.data = address_space
                        .read_le_value::<_, false>(self.state.bus.address as Address, &timestamp)
                        .unwrap_or_default();

                    true
                }
                BusMode::Write => false,
            };

            if self.state.rdy || !is_read_cycle {
                if core::mem::take(&mut self.state.consume_effective_address) {
                    self.state.effective_address.clear();
                }

                let current_cycle = self.state.cycle_queue.pop_front().unwrap();

                self.handle_phi2(&current_cycle);

                match current_cycle.bus_mode {
                    BusMode::Read => {}
                    BusMode::Write => {
                        address_space
                            .write_le_value(
                                self.state.bus.address as Address,
                                &timestamp,
                                self.state.bus.data,
                            )
                            .unwrap_or_default();
                    }
                }

                // Check for interrupts

                if V::SUPPORTS_INTERRUPTS && self.state.cycle_queue.is_empty() {
                    if self.state.nmi.interrupt_required() {
                        self.handle_nmi();
                    } else if core::mem::take(&mut self.state.irq) {
                        self.handle_irq();
                    }
                }
            }
        }
    }
}
