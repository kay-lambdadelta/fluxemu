use fluxemu_system::testing::assert_snapshots_reproducible;
use fluxemu_system_other_chip8::Chip8;

#[path = "../test_roms.rs"]
mod test_roms;

#[test]
fn snapshots_are_reproducible() {
    assert_snapshots_reproducible(Chip8, test_roms::TEST_ROMS);
}
