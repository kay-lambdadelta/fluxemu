use fluxemu_frontend::machine::FactoryManager;
use fluxemu_runtime::platform::Platform;
use fluxemu_system_atari_2600::Atari2600;
use fluxemu_system_atari_lynx::AtariLynx;
use fluxemu_system_nintendo_gameboy::Gameboy;
use fluxemu_system_nintendo_nes::Nes;
use fluxemu_system_other_chip8::Chip8;

#[cfg(feature = "webgpu")]
pub fn get_webgpu_factories<P: Platform<GraphicsApi = fluxemu_graphics::api::webgpu::Webgpu>>()
-> FactoryManager<P> {
    let mut factories = FactoryManager::default();

    factories.insert_factory::<Atari2600>();
    factories.insert_factory::<AtariLynx>();
    factories.insert_factory::<Chip8>();
    factories.insert_factory::<Nes>();
    factories.insert_factory::<Gameboy>();

    factories
}

pub fn get_software_factories<
    P: Platform<GraphicsApi = fluxemu_graphics::api::software::Software>,
>() -> FactoryManager<P> {
    let mut factories = FactoryManager::default();

    factories.insert_factory::<Atari2600>();
    factories.insert_factory::<AtariLynx>();
    factories.insert_factory::<Chip8>();
    factories.insert_factory::<Nes>();
    factories.insert_factory::<Gameboy>();

    factories
}
