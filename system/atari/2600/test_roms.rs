use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "Donkey Kong (USA).a26",
        id: "73ebbb4cb8c75ca45a1cf93be5108868190d09e3509e77a4fa74207a8e179966",
    },
    TestRom::Stored {
        name: "Ms. Pac-Man (USA).a26",
        id: "3d80d662e8ecfadc7f03746e8860e6f22c9e4e3a5c1d6e22594c1ffeebd937a6",
    },
    TestRom::Stored {
        name: "Asteroids (USA).a26",
        id: "8008efc96b99eddc57c6321fb2012ccd43caaa9d366dac278877124431b4ebcd",
    },
];
