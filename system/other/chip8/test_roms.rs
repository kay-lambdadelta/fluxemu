use fluxemu_system::testing::TestRom;

pub const TEST_ROMS: &[TestRom] = &[
    TestRom::Stored {
        name: "octojam2title.ch8",
        id: "cbc11026d03e6270658dd2ec40f216cfef6d6b45da0a199ef77e33970f52788d",
    },
    TestRom::Stored {
        name: "horseWorldOnline.ch8",
        id: "9a0bdf317592a759972162a0ada50abd9359f0d9803b2d71dfc262d4e7ac2170",
    },
    TestRom::Stored {
        name: "octorancher.ch8",
        id: "1ac5f53e17d79107bc3eb0901235b94615fdf68bfb20d8295be519296fb59d95",
    },
    TestRom::Stored {
        name: "slipperyslope.ch8",
        id: "e2f606d0d99f69024ecbd5fc509a6830077e9235472aa19952702ac1bcf8a562",
    },
    TestRom::Stored {
        name: "Jumping X and O [Harry Kleinberg, 1977].ch8",
        id: "43023d2190d4e649fc6589b9d4fc63cca0d55f01178133edace18edf5c7035b1",
    },
    TestRom::Stored {
        name: "Rocket [Joseph Weisbecker, 1978].ch8",
        id: "9c291f6021ca3e25ec7bdd66b11ebd98c6167b8cf83f17320f855113302ca834",
    },
    TestRom::Stored {
        name: "Kaleidoscope [Joseph Weisbecker, 1978].ch8",
        id: "ea35ea7224ad350a60d83e0c7da15b658a7cbb3d4e1a57479d2a659431927736",
    },
    TestRom::Stored {
        name: "Shooting Stars [Philip Baltzer, 1978].ch8",
        id: "b8a1f7e2794ce702c78f4f8452fe0e348698de56aa822b441ebfe9d2de6f1970",
    },
    TestRom::Stored {
        name: "Breakout [Carmelo Cortez, 1979].ch8",
        id: "a952c0848cb69ed2202f50ebaa7ccd909009497435fe5a6d135331cd13c769ee",
    },
    TestRom::Stored {
        name: "Wipe Off [Joseph Weisbecker].ch8",
        id: "87f0a6bdc1022f1286fb8183e5a695123dbf203c610a5470288a4820243e1764",
    },
];
