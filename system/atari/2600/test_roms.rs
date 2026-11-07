use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "Donkey Kong (USA).a26",
        id: "6e6e37ec8d66aea1c13ed444863e3db91497aa35",
    },
    TestRom::Stored {
        name: "Ms. Pac-Man (USA).a26",
        id: "62b933cdd8844bb1816ce57889203954fe782603",
    },
    TestRom::Stored {
        name: "Asteroids (USA).a26",
        id: "8423f99092b454aed89f89f5d7da658caf7af016",
    },
];
