use fluxemu_runtime::ComponentPath;
use fluxemu_system::testing::{TestRom, read_blargg_status, run_until};
use fluxemu_system_nintendo_nes::Nes;

const FRAME_LIMIT: u32 = 60 * 120;
const RESULT_ADDRESS: usize = 0x6000;

#[test]
fn all_instrs() {
    let rom = TestRom::Embedded {
        file_name: "all_instrs.nes",
        data: include_bytes!("../../../../external/nes-test-roms/instr_test-v5/all_instrs.nes"),
    };
    let cpu = ComponentPath::new("cpu").unwrap();

    let mut status = None;

    let result = run_until(Nes, &rom, FRAME_LIMIT, |runtime| {
        let address_space = runtime.bus_master_address_spaces(&cpu).next().unwrap();
        status = read_blargg_status(runtime, address_space, RESULT_ADDRESS);

        status.clone().filter(|status| !status.is_running())
    });

    let Some(result) = result else {
        match status {
            Some(status) => panic!(
                "all_instrs.nes: no result after {FRAME_LIMIT} frames, last output:\n{}",
                status.text
            ),
            None => panic!("all_instrs.nes: never reported a status in {FRAME_LIMIT} frames"),
        }
    };

    assert!(
        result.code == 0,
        "all_instrs.nes failed with code {}:\n{}",
        result.code,
        result.text
    );
}
