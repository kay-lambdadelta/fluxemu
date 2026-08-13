use fluxemu_frontend::machine::FactoryManager;
use fluxemu_graphics::api::software::Software;
use fluxemu_runtime::platform::Platform;
use fluxemu_system_atari_2600::Atari2600;
use fluxemu_system_nintendo_nes::Nes;
use fluxemu_system_other_chip8::Chip8;

pub fn get_software_factories<P: Platform<GraphicsApi = Software>>() -> FactoryManager<P> {
    let mut factories = FactoryManager::default();

    factories.insert_factory::<Atari2600>();
    factories.insert_factory::<Chip8>();
    factories.insert_factory::<Nes>();

    factories
}
