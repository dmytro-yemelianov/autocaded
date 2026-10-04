use acad_translated::runtime::{self, Trap};
use std::{fs, path::Path};

struct BootState {
    result: Result<u64, Trap>,
    memory: Vec<u8>,
    registers: [u8; 8192],
    console: Vec<u8>,
    exit_code: Option<u8>,
    remaining_budget: usize,
}

fn boot(input: &[u8]) -> BootState {
    let input = input.to_vec();
    std::thread::Builder::new()
        .name("translated_boot".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let mut memory = vec![0; 1024 * 1024];
            let mut registers = [0; 8192];
            runtime::initialize_text_video(&mut memory).unwrap();
            let image_dir = std::env::var("ACAD_RE_IMAGES")
                .unwrap_or_else(|_| "../../target/acad-ir-20261002-01/input".into());
            let exe = fs::read(Path::new(&image_dir).join("ACAD.EXE")).unwrap();
            memory[0x10100..0x10100 + exe.len() - 512].copy_from_slice(&exe[512..]);

            runtime::reset_dos();
            let system_dir =
                std::env::var("ACAD_RE_SYSTEM").unwrap_or_else(|_| "../../corpus/System".into());
            for entry in fs::read_dir(&system_dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_file() {
                    runtime::mount_file(
                        path.file_name().unwrap().to_str().unwrap(),
                        fs::read(&path).unwrap(),
                    );
                }
            }
            runtime::queue_input_bytes(&input);
            runtime::DOS_ENV.with(|env| env.borrow_mut().yield_on_empty_input = true);

            // DOS PSP at 1000:0000 and resident image at 1010:0000.
            runtime::store(&mut memory, 0x10002, 2, 0x9fff).unwrap();
            memory[0x10080] = 0;
            memory[0x10081] = b'\r';
            runtime::write(&mut registers, 258, 2, 0x1000);
            runtime::write(&mut registers, 260, 2, 0x1e25);
            runtime::write(&mut registers, 262, 2, 0x1000);
            runtime::write(&mut registers, 256, 2, 0x1000);
            runtime::write(&mut registers, 16, 2, 0x51a0);
            let mut remaining_budget = 1_000_000;
            let result = acad_translated::exe_code::fn_exe_0100(
                &mut registers,
                &mut memory,
                &mut remaining_budget,
            );
            let console = runtime::get_console_output();
            let exit_code = runtime::DOS_ENV.with(|env| env.borrow().exit_code);
            println!(
                "Boot result: {result:?}; instructions: {}\n{}",
                1_000_000 - remaining_budget,
                String::from_utf8_lossy(&console)
            );
            // Capture provenance on the same thread as the hosted DOS state.
            let code_segment = runtime::load(&memory, 0x1e250 + 0x4d70, 2).unwrap();
            let code_address = (code_segment << 4) + 0x100;
            if input.is_empty() {
                assert_eq!(runtime::loaded_overlay_offset(code_address), Some(0x9380));
                let ovl = fs::read(Path::new(&system_dir).join("ACAD.OVL")).unwrap();
                assert_eq!(
                    &memory[code_address as usize..code_address as usize + 0x4fa0],
                    &ovl[0x9380..0x9380 + 0x4fa0]
                );
            }
            BootState {
                result,
                memory,
                registers,
                console,
                exit_code,
                remaining_budget,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn boot_reaches_the_main_menu_and_waits_for_input() {
    let state = boot(b"");
    assert_eq!(state.result, Err(Trap::InputRequired));
    assert!(state.remaining_budget > 0);
    assert_eq!(state.exit_code, None);
    let console = String::from_utf8_lossy(&state.console);
    assert!(console.contains("Main Menu"));
    assert!(console.contains("0.  Exit AutoCAD"));
    assert!(console.ends_with("Enter selection: "));
    assert!(!console.contains("REDIRECTION ERROR"));
    assert_eq!(runtime::read(&state.registers, 258, 2), 0x1000);
    assert_eq!(runtime::read(&state.registers, 0, 2), 0x0600);
    assert_eq!(runtime::read(&state.registers, 518, 1), 1);
    assert_eq!(
        runtime::load(&state.memory, 0x1e250 + 0x4d70, 2),
        Ok(0x2abb)
    );
}

#[test]
fn boot_accepts_the_menu_exit_selection() {
    let state = boot(b"0\r");
    assert_eq!(state.result, Err(Trap::Halt));
    assert_eq!(state.exit_code, Some(0));
    assert!(state.remaining_budget > 0);
    assert!(String::from_utf8_lossy(&state.console).contains("Main Menu"));
}
