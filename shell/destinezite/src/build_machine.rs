use fluxemu_frontend::machine::FactoryManager;
use fluxemu_platform::Platform;
use fluxemu_system_atari_2600::Atari2600;
use fluxemu_system_atari_lynx::AtariLynx;
use fluxemu_system_nintendo_gameboy::Gameboy;
use fluxemu_system_nintendo_nes::Nes;
use fluxemu_system_other_chip8::Chip8;

pub fn get_factories<P: Platform>() -> FactoryManager<P> {
    let mut factories = FactoryManager::default();

    factories.insert::<Atari2600>();
    factories.insert::<AtariLynx>();
    factories.insert::<Chip8>();
    factories.insert::<Nes>();
    factories.insert::<Gameboy>();

    factories
}
