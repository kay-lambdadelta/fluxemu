use super::{AtariSystem, NintendoSystem, OtherSystem, SegaSystem, SonySystem, SystemId};

// TODO: This should factor in rom format to handle the more tricky formats

pub fn get_extensions(system: SystemId) -> &'static [&'static str] {
    match system {
        SystemId::Nintendo(NintendoSystem::GameBoy) => &["gb"],
        SystemId::Nintendo(NintendoSystem::GameBoyColor) => &["gbc"],
        SystemId::Nintendo(NintendoSystem::GameBoyAdvance) => &["gba"],
        SystemId::Nintendo(NintendoSystem::GameCube) => &["iso"],
        SystemId::Nintendo(NintendoSystem::Wii) => &["iso"],
        SystemId::Nintendo(NintendoSystem::NintendoEntertainmentSystem) => &["nes"],
        SystemId::Nintendo(NintendoSystem::SuperNintendoEntertainmentSystem) => &["sfc", "smc"],
        SystemId::Nintendo(NintendoSystem::Nintendo64) => &["z64", "n64"],
        SystemId::Sega(SegaSystem::GameGear) => &["gg"],
        SystemId::Sega(SegaSystem::MasterSystem) => &["sms"],
        SystemId::Sega(SegaSystem::Genesis) => &["md"],
        SystemId::Sega(SegaSystem::Sega32X) => &["32x"],
        SystemId::Sega(SegaSystem::SegaCD) => &["iso"],
        SystemId::Sony(SonySystem::PlaystationPortable) => &["iso"],
        SystemId::Atari(AtariSystem::_2600) => &["a26"],
        SystemId::Atari(AtariSystem::_5200) => &["a52"],
        SystemId::Atari(AtariSystem::_7800) => &["a78"],
        SystemId::Atari(AtariSystem::Lynx) => &["lnx"],
        SystemId::Atari(AtariSystem::Jaguar) => &["jag"],
        SystemId::Other(OtherSystem::Chip8) => &["ch8", "c8"],
        _ => &[],
    }
}
