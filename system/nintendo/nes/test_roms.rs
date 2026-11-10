use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "Legend of Zelda, The (USA) (Rev 1).nes",
        id: "7c3c726d8193389c2515fed9deef34c24c6c3e4f6ff45a92bc7202f78965e7fe",
    },
    TestRom::Stored {
        name: "Metroid (USA).nes",
        id: "64291dc223c3d30168821dc2e2ea1eefba21aad18f80dd37856fe8ef3acf106e",
    },
    TestRom::Stored {
        name: "Super Mario Bros. (World).nes",
        id: "3cff9c1c6453768051fe26e6b85ac3463cfe88b73ca48f9b0e5bed3bdcce928e",
    },
    TestRom::Stored {
        name: "Gradius (USA).nes",
        id: "ffd50e15aa94b907e3fbe735291f69f786f954d7ced5c203cdb648e96c3b0bff",
    },
    TestRom::Stored {
        name: "Solomon's Key (USA).nes",
        id: "745b4ffbc937e6f02a8d0f1613f0f0d30cc478a292187e256e93ad086f683bf8",
    },
    TestRom::Stored {
        name: "Blaster Master (USA).nes",
        id: "13286372a770ceff8e0675d5b4a6fcead24692bfec8c972b03f34e6b789af3ba",
    },
    TestRom::Stored {
        name: "Cybernoid - The Fighting Machine (USA).nes",
        id: "207c72f6c3372477688e639b580385c2be5153f6c3930ae7461478c7ab4d5122",
    },
    TestRom::Stored {
        name: "Pennant League!! - Home Run Nighter (Japan).nes",
        id: "1c575d644d73ce8296897f07947885f590543f1c5f142cfb9578a8e0fac16bed",
    },
    TestRom::Stored {
        name: "Hydlide (USA).nes",
        id: "afdf9890fcfabb98e6a34427b649c0551eb2a41234ec2eabf002efe8784787c2",
    },
    TestRom::Stored {
        name: "Pac-Man (USA) (Tengen).nes",
        id: "a1c1f6b5d11c4d43c063ee225776880cbb9eb56249e870500c7ace2f540eee08",
    },
    TestRom::Stored {
        name: "1942 (Japan, USA) (En).nes",
        id: "7597a23addcea45345cb3f9b2fadbc92618d2a902e1e7a39442d02034e624d29",
    },
    TestRom::Stored {
        name: "Xevious (Japan) (En).nes",
        id: "1a95917e12cfdfa5f066f77fe99c14a47fbeabc61ad74904e95a37d8b0b9a45f",
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
