use fluxemu_system::testing::assert_snapshots_reproducible;
use fluxemu_system_atari_2600::Atari2600;

#[path = "../test_roms.rs"]
mod test_roms;

#[test]
fn snapshots_are_reproducible() {
    assert_snapshots_reproducible(Atari2600, test_roms::TEST_ROMS);
}
