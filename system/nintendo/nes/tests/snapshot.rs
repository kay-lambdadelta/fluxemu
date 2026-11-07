use fluxemu_system::testing::assert_snapshots_reproducible;
use fluxemu_system_nintendo_nes::Nes;

#[path = "../test_roms.rs"]
mod test_roms;

#[test]
fn snapshots_are_reproducible() {
    assert_snapshots_reproducible(Nes, test_roms::TEST_ROMS);
}
