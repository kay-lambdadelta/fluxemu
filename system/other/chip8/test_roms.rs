use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "octojam2title.ch8",
        id: "11c68038d64a09be549a6c1e50724808914d8991",
    },
    TestRom::Stored {
        name: "horseWorldOnline.ch8",
        id: "28ac3467fbb4544a3e3a1ec3cd27d9e819ac7323",
    },
    TestRom::Stored {
        name: "octorancher.ch8",
        id: "8263bac7d98d94097171f0a5dc6f210f77543080",
    },
    TestRom::Stored {
        name: "slipperyslope.ch8",
        id: "9d834860f455aec7e95fb886984497e5be501610",
    },
    TestRom::Stored {
        name: "Jumping X and O [Harry Kleinberg, 1977].ch8",
        id: "5b29263763be401c31d805bc35a4cd211d552881",
    },
    TestRom::Stored {
        name: "Rocket [Joseph Weisbecker, 1978].ch8",
        id: "3d1d029d6e31206d245c0ba881c0d1f003953bad",
    },
    TestRom::Stored {
        name: "Kaleidoscope [Joseph Weisbecker, 1978].ch8",
        id: "fc724ae0125f5f1ac94a79fe3afc6318b1f57556",
    },
    TestRom::Stored {
        name: "Shooting Stars [Philip Baltzer, 1978].ch8",
        id: "443550abf646bc7f475ef0466f8e1232ec7474f3",
    },
    TestRom::Stored {
        name: "Breakout [Carmelo Cortez, 1979].ch8",
        id: "193915dcde1365ae054c4eaa21a35baa27cd3356",
    },
    TestRom::Stored {
        name: "Wipe Off [Joseph Weisbecker].ch8",
        id: "d666688a8fce468a7d88b536bc1ef5f35ba12031",
    },
];
