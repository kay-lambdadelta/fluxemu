use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "Legend of Zelda, The (USA) (Rev 1).nes",
        id: "d798f17e98a3e9ef16a71c8372d59b55fe0299d3e65df690ebb875971e42f637",
    },
    TestRom::Stored {
        name: "Metroid (USA).nes",
        id: "eb4d76a54c6cadb57da2fc18465e1039eb7bfcff354e38efc7b12f928f41f54d",
    },
    TestRom::Stored {
        name: "Super Mario Bros. (World).nes",
        id: "7370833e6c26479c5b3b9548740a15aa8068b7cc244934803a26c58b6e8a8c46",
    },
    TestRom::Stored {
        name: "Gradius (USA).nes",
        id: "ba4364e5bd33901da71a511fb5acedbb1eee903183773371d6a03d3b3aa1844d",
    },
    TestRom::Stored {
        name: "Solomon's Key (USA).nes",
        id: "18d7918fe88ddc2958320870ea0584d2533370e0b1db8b196123cb0db2c077cd",
    },
    TestRom::Stored {
        name: "Blaster Master (USA).nes",
        id: "e307f9bbe09e49854ba90ba928d417d38463543995d05cf550609446f9e27877",
    },
    TestRom::Stored {
        name: "Cybernoid - The Fighting Machine (USA).nes",
        id: "eb858bdeccd77305db9e5f8d8a03c816a8fdca7590c984b1f903e7710f70d1c0",
    },
    TestRom::Stored {
        name: "Pennant League!! - Home Run Nighter (Japan).nes",
        id: "11525e3fd5359150fe48f405c1cc9cbd61613f7099bd708c240b399afe728eae",
    },
    TestRom::Stored {
        name: "Hydlide (USA).nes",
        id: "2fbf629d7baa98c432c7e746dfc14d6a0441629fe3e70ce993af38ecab73463a",
    },
    TestRom::Stored {
        name: "Pac-Man (USA) (Tengen).nes",
        id: "c2af6c0f5894e1060d779e1a015f15ed076a6ec9820937fbeb6b07160ef828b9",
    },
    TestRom::Stored {
        name: "1942 (Japan, USA) (En).nes",
        id: "18d1fd4573d3bfc528efe0d7a1208a4d3e6edf316b5cddedbc24c65de3596c69",
    },
    TestRom::Stored {
        name: "Xevious (Japan) (En).nes",
        id: "3e77af27725c73072ba1256f0374557c9e69aed6d5c8308f6979e6cbd2aade1b",
    },
    TestRom::Embedded {
        file_name: "StarsSE.NES",
        data: include_bytes!("../../../external/nes-test-roms/stars_se/StarsSE.NES"),
    },
    TestRom::Embedded {
        file_name: "all_instrs.nes",
        data: include_bytes!("../../../external/nes-test-roms/instr_test-v5/all_instrs.nes"),
    },
    TestRom::Embedded {
        file_name: "FLAME.NES",
        data: include_bytes!("../../../external/nes-test-roms/other/FLAME.NES"),
    },
    TestRom::Embedded {
        file_name: "ny2011.nes",
        data: include_bytes!("../../../external/nes-test-roms/ny2011/ny2011.nes"),
    },
    TestRom::Embedded {
        file_name: "SPRITE.NES",
        data: include_bytes!("../../../external/nes-test-roms/other/SPRITE.NES"),
    },
];
