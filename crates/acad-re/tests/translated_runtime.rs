#[allow(dead_code)]
#[path = "../src/translated_runtime.rs"]
mod runtime;

use runtime::*;

fn dispatch_address(
    block: &str,
    target: u64,
    registers: &mut [u8; 8192],
    memory: &mut [u8],
    budget: &mut usize,
) -> Result<u64, Trap> {
    if block == "EXE_CODE" && target == 0x14567 {
        assert_eq!(read(registers, 258, 2), 0x1000);
        assert_eq!(read(registers, 24, 2), 0x4567);
        let stack = segment(read(registers, 260, 2), read(registers, 16, 2));
        assert_eq!(load(memory, stack, 2), Ok(0x8f92));
        *budget = budget.checked_sub(1).ok_or(Trap::Budget)?;
        let sp = read(registers, 16, 2) + 2;
        write(registers, 16, 2, sp);
        write(registers, 0, 2, 42);
        write(registers, 24, 2, 0xbeef);
        return Ok(0x8f92);
    }
    if block != "EXE_CODE" || target != 0x12345 {
        return Err(Trap::InvalidPc);
    }
    *budget = budget.checked_sub(1).ok_or(Trap::Budget)?;
    Ok(0x54321)
}

fn input_registers() -> [u8; 8192] {
    let mut registers = [0; 8192];
    write(&mut registers, 0, 2, 0x0a00);
    write(&mut registers, 262, 2, 0x10);
    write(&mut registers, 8, 2, 0x20);
    registers
}

#[test]
fn buffered_input_truncates_and_returns_dos_count_and_carriage_return() {
    reset_dos();
    queue_input_line(b"abcdef\rignored");
    queue_input_line(b"\n");
    let mut registers = input_registers();
    let expected_registers = registers;
    let mut memory = vec![0x55; 0x200];
    memory[0x120] = 4;
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(&memory[0x120..0x126], &[4, 3, b'a', b'b', b'c', b'\r']);
    assert_eq!(memory[0x126], 0x55);
    assert_eq!(registers, expected_registers);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(&memory[0x121..0x123], &[0, b'\r']);
    assert_eq!(get_console_output(), b"abc\r\r");
}

#[test]
fn buffered_input_without_a_line_preserves_memory_and_registers() {
    reset_dos();
    let mut registers = input_registers();
    let expected_registers = registers;
    let mut memory = vec![0x55; 0x200];
    memory[0x120] = 4;
    let expected_memory = memory.clone();
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut memory),
        Err(Trap::InputRequired)
    );
    assert_eq!(registers, expected_registers);
    assert_eq!(memory, expected_memory);
}

#[test]
fn buffered_input_validates_the_buffer_before_consuming_a_line() {
    reset_dos();
    queue_input_line(b"a");
    let mut registers = input_registers();
    let mut memory = vec![0; 0x124];
    memory[0x120] = 4;
    let expected = memory.clone();
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut memory),
        Err(Trap::Memory)
    );
    assert_eq!(memory, expected);
    memory.resize(0x126, 0);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(&memory[0x121..0x124], &[1, b'a', b'\r']);
    memory[0x120] = 0;
    assert_eq!(host_interrupt(0x21, &mut registers, &mut memory), Ok(0));
}

#[test]
fn indirect_dispatch_preserves_return_values_and_propagates_failures() {
    let mut registers = [0; 8192];
    let mut memory = [];
    let mut budget = 2;
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            0x12345
        ),
        Ok(0x54321)
    );
    assert_eq!(
        dispatch_indirect_jump(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            0x12345
        ),
        Ok(0x54321)
    );
    assert_eq!(budget, 0);
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            0x12345
        ),
        Err(Trap::Budget)
    );
    assert_eq!(
        dispatch_indirect(&mut registers, &mut memory, &mut budget, "EXE_CODE", 0),
        Err(Trap::InvalidPc)
    );
    assert_eq!(
        dispatch_call(&mut registers, &mut memory, &mut budget, 0),
        Err(Trap::InvalidPc)
    );
}

#[test]
fn unsupported_interrupts_and_services_fail_explicitly() {
    let mut registers = [0; 8192];
    assert_eq!(
        host_interrupt(0x42, &mut registers, &mut []),
        Err(Trap::UnsupportedInterrupt)
    );
    write(&mut registers, 0, 2, 0xff00);
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut []),
        Err(Trap::UnsupportedInterrupt)
    );
}

fn record_registers(memory: &mut [u8], function: u64) -> [u8; 8192] {
    let mut registers = input_registers();
    write(&mut registers, 0, 2, function << 8);
    memory[0x121..0x12c].copy_from_slice(&to_fcb_name("DATA.BIN"));
    store(memory, 0x12e, 2, 4).unwrap();
    DOS_ENV.with(|env| env.borrow_mut().dta = 0x200);
    registers
}

#[test]
fn fcb_block_read_reports_partial_record_and_updates_position() {
    reset_dos();
    mount_file("DATA.BIN", b"abcde".to_vec());
    let mut memory = vec![0; 0x300];
    let mut registers = record_registers(&mut memory, 0x0f);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(read(&registers, 0, 1), 0);
    assert_eq!(load(&memory, 0x130, 4), Ok(5));
    write(&mut registers, 0, 2, 0x2700);
    write(&mut registers, 4, 2, 2);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(&memory[0x200..0x208], b"abcde\0\0\0");
    assert_eq!(read(&registers, 0, 1), 3);
    assert_eq!(read(&registers, 4, 2), 2);
    assert_eq!(load(&memory, 0x141, 4), Ok(2));
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(read(&registers, 0, 1), 1);
    assert_eq!(read(&registers, 4, 2), 0);
}

#[test]
fn fcb_transfer_and_struct_bounds_return_memory_traps() {
    reset_dos();
    mount_file("DATA.BIN", b"a".to_vec());
    let mut memory = vec![0; 0x203];
    let mut registers = record_registers(&mut memory, 0x21);
    let before = memory.clone();
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut memory),
        Err(Trap::Memory)
    );
    assert_eq!(memory, before);
    write(&mut registers, 0, 2, 0x2800);
    write(&mut registers, 4, 2, 2);
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut memory),
        Err(Trap::Memory)
    );
    assert_eq!(memory, before);
    write(&mut registers, 0, 2, 0x0f00);
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut memory[..0x130]),
        Err(Trap::Memory)
    );
}

#[test]
fn overlay_provenance_tracks_relocations_replacement_and_eof_padding() {
    reset_dos();
    mount_file("ACAD.OVL", (0..13).collect());
    mount_file("DATA.BIN", vec![0xff; 4]);
    let mut memory = vec![0; 0x400];
    let mut registers = record_registers(&mut memory, 0x27);
    memory[0x121..0x12c].copy_from_slice(&to_fcb_name("ACAD.OVL"));
    write(&mut registers, 4, 2, 3);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(loaded_overlay_offset(0x200), Some(0));
    assert_eq!(loaded_overlay_offset(0x20b), Some(11));
    assert_eq!(loaded_overlay_offset(0x20c), None);

    // Overwriting the middle with another file preserves both outer ranges.
    memory[0x121..0x12c].copy_from_slice(&to_fcb_name("DATA.BIN"));
    store(&mut memory, 0x141, 4, 0).unwrap();
    DOS_ENV.with(|env| env.borrow_mut().dta = 0x204);
    write(&mut registers, 0, 2, 0x2700);
    write(&mut registers, 4, 2, 1);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(loaded_overlay_offset(0x203), Some(3));
    assert_eq!(loaded_overlay_offset(0x204), None);
    assert_eq!(loaded_overlay_offset(0x208), Some(8));

    // Loading a new overlay range replaces old offsets and excludes padding.
    memory[0x121..0x12c].copy_from_slice(&to_fcb_name("ACAD.OVL"));
    store(&mut memory, 0x141, 4, 3).unwrap();
    write(&mut registers, 0, 2, 0x2700);
    host_interrupt(0x21, &mut registers, &mut memory).unwrap();
    assert_eq!(&memory[0x204..0x208], &[12, 0, 0, 0]);
    assert_eq!(loaded_overlay_offset(0x204), Some(12));
    assert_eq!(loaded_overlay_offset(0x205), None);
    assert_eq!(loaded_overlay_offset(0x208), Some(8));
    reset_dos();
    assert_eq!(loaded_overlay_offset(0x200), None);
}

#[test]
fn kernel_bridge_switches_segments_and_preserves_its_return_stack() {
    let mut registers = [0; 8192];
    let mut memory = vec![0; 0x30000];
    write(&mut registers, 262, 2, 0x1e25);
    write(&mut registers, 260, 2, 0x1e25);
    write(&mut registers, 258, 2, 0x2abb);
    write(&mut registers, 16, 2, 0x5100);
    let ds = 0x1e250;
    store(&mut memory, ds + 0x4d76, 2, 0x8f80).unwrap();
    store(&mut memory, ds + 0x4d78, 2, 0x1000).unwrap();
    store(&mut memory, ds + 0x4d74, 2, 0x2abb).unwrap();
    store(&mut memory, ds + 0x4d86, 2, 4).unwrap();
    store(&mut memory, ds + 0x5100, 2, 0x5c4).unwrap();
    let mut budget = 1;
    assert_eq!(
        dispatch_kernel_tailcall(&mut registers, &mut memory, &mut budget, 0x4567),
        Ok(0x2b174)
    );
    assert_eq!(budget, 0);
    assert_eq!(read(&registers, 0, 2), 42);
    assert_eq!(read(&registers, 258, 2), 0x2abb);
    assert_eq!(read(&registers, 16, 2), 0x5102);
    assert_eq!(read(&registers, 12, 2), 2);
    assert_eq!(read(&registers, 24, 2), 4);
    assert_eq!(read(&registers, 28, 2), 0x5c4);
    assert_eq!(load(&memory, ds + 0x4d80, 2), Ok(0x5c4));
    assert_eq!(load(&memory, ds + 0x4d86, 2), Ok(4));
    assert_eq!(load(&memory, ds + 0x4d72, 2), Ok(0x5c4));

    // A resident trap leaves the machine in resident CS for diagnosis.
    store(&mut memory, ds + 0x5102, 2, 0x5c4).unwrap();
    assert_eq!(
        dispatch_kernel_tailcall(&mut registers, &mut memory, &mut budget, 0x4567),
        Err(Trap::Budget)
    );
    assert_eq!(read(&registers, 258, 2), 0x1000);
    assert_eq!(load(&memory, ds + 0x4d86, 2), Ok(2));
    store(&mut memory, ds + 0x4d76, 2, 0).unwrap();
    assert_eq!(
        dispatch_kernel_tailcall(&mut registers, &mut memory, &mut budget, 0x4567),
        Err(Trap::InvalidPc)
    );
}

#[test]
fn far_returns_only_match_the_active_caller_and_scopes_unwind_on_traps() {
    let mut registers = [0; 8192];
    let mut memory = vec![0; 0x200];
    let mut budget = 10;
    write(&mut registers, 258, 2, 0x1000);
    write(&mut registers, 260, 2, 0x10);
    write(&mut registers, 16, 2, 0x20);
    store(&mut memory, 0x120, 2, 0x425).unwrap();
    store(&mut memory, 0x122, 2, 0x777).unwrap();
    let outer = 0x10425;
    let inner = 0x10777;
    assert_eq!(
        call_with_return(
            &mut registers,
            &mut memory,
            &mut budget,
            |registers, memory, budget| {
                write(registers, 16, 2, 0x22);
                assert_eq!(
                    call_with_return(registers, memory, budget, |registers, memory, budget| {
                        assert_eq!(
                            dispatch_indirect_jump(registers, memory, budget, "OVL00_CODE", outer),
                            Err(Trap::InvalidPc)
                        );
                        dispatch_indirect_jump(registers, memory, budget, "OVL00_CODE", inner)
                    }),
                    Ok(inner)
                );
                dispatch_indirect_jump(registers, memory, budget, "OVL00_CODE", outer)
            }
        ),
        Ok(outer)
    );
    assert_eq!(
        dispatch_indirect_jump(
            &mut registers,
            &mut memory,
            &mut budget,
            "OVL00_CODE",
            outer
        ),
        Err(Trap::InvalidPc)
    );
    assert_eq!(
        call_with_return(&mut registers, &mut memory, &mut budget, |_, _, _| Err(
            Trap::Budget
        )),
        Err(Trap::Budget)
    );
    assert_eq!(
        dispatch_indirect_jump(
            &mut registers,
            &mut memory,
            &mut budget,
            "OVL00_CODE",
            inner
        ),
        Err(Trap::InvalidPc)
    );
    assert_eq!(budget, 10);
}

#[test]
fn bios_scroll_changes_only_the_requested_text_window() {
    let mut registers = [0; 8192];
    let mut memory = vec![0x55; 0xc0000];
    for row in 0..25 {
        for col in 0..80 {
            let off = 0xb8000 + (row * 80 + col) * 2;
            memory[off] = b'A' + row as u8;
            memory[off + 1] = 0x1f;
        }
    }
    write(&mut registers, 0, 2, 0x0601);
    write(&mut registers, 12, 2, 0x0700);
    write(&mut registers, 4, 2, 0x0203);
    write(&mut registers, 8, 2, 0x0405);
    let expected_registers = registers;
    let before = memory.clone();
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    for row in 0..25 {
        for col in 0..80 {
            let off = 0xb8000 + (row * 80 + col) * 2;
            let expected = if (2..=4).contains(&row) && (3..=5).contains(&col) {
                if row == 4 {
                    [b' ', 7]
                } else {
                    [b'A' + row as u8 + 1, 0x1f]
                }
            } else {
                [before[off], before[off + 1]]
            };
            assert_eq!(&memory[off..off + 2], &expected);
        }
    }
    assert_eq!(registers, expected_registers);
    write(&mut registers, 0, 2, 0x0600);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    assert_eq!(
        &memory[0xb8000 + (2 * 80 + 3) * 2..0xb8000 + (2 * 80 + 3) * 2 + 2],
        b" \x07"
    );
}

#[test]
fn generated_interrupt_stub_checks_bytes_budget_and_far_return_stack() {
    let mut registers = [0; 8192];
    let mut memory = vec![0; 0xc0000];
    let mut budget = 2;
    write(&mut registers, 262, 2, 0x1e25);
    write(&mut registers, 258, 2, 0x1e25);
    write(&mut registers, 260, 2, 0x10);
    write(&mut registers, 16, 2, 0x20);
    write(&mut registers, 0, 2, 0x0600);
    write(&mut registers, 12, 2, 0x0700);
    write(&mut registers, 8, 2, 0x184f);
    let stub = 0x2306a;
    memory[stub..stub + 3].copy_from_slice(&[0xcd, 0x10, 0xcb]);
    store(&mut memory, 0x120, 2, 0x94a8).unwrap();
    store(&mut memory, 0x122, 2, 0x1000).unwrap();
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            stub as u64
        ),
        Ok(0x194a8)
    );
    assert_eq!(budget, 0);
    assert_eq!(read(&registers, 16, 2), 0x24);
    assert_eq!(read(&registers, 258, 2), 0x1000);
    assert!(memory[0xb8000..0xb8000 + 4000]
        .chunks_exact(2)
        .all(|cell| cell == b" \x07"));
    write(&mut registers, 258, 2, 0x1e25);
    write(&mut registers, 16, 2, 0x20);
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            stub as u64
        ),
        Err(Trap::Budget)
    );
    budget = 2;
    memory[stub + 2] = 0xc3;
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            stub as u64
        ),
        Err(Trap::InvalidPc)
    );
    assert_eq!(budget, 2);
    memory[stub + 2] = 0xcb;
    memory[stub + 1] = 0x42;
    assert_eq!(
        dispatch_indirect(
            &mut registers,
            &mut memory,
            &mut budget,
            "EXE_CODE",
            stub as u64
        ),
        Err(Trap::UnsupportedInterrupt)
    );
    assert_eq!(budget, 1);
    assert_eq!(read(&registers, 258, 2), 0x1e25);
    assert_eq!(read(&registers, 16, 2), 0x20);
}

#[test]
fn text_video_cursor_character_and_mode_services_share_the_bios_data() {
    reset_dos();
    let mut memory = vec![0; 0xc0000];
    let mut registers = [0; 8192];
    initialize_text_video(&mut memory).unwrap();
    write(&mut registers, 0, 2, 0x0200);
    write(&mut registers, 8, 2, 0x0304);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    write(&mut registers, 0, 2, 0x01ff);
    write(&mut registers, 4, 2, 0x2000);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    write(&mut registers, 0, 2, 0x0300);
    write(&mut registers, 4, 2, 0);
    write(&mut registers, 8, 2, 0);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    assert_eq!(read(&registers, 4, 2), 0x2000);
    assert_eq!(read(&registers, 8, 2), 0x0304);
    write(&mut registers, 0, 2, 0x0958);
    write(&mut registers, 12, 2, 0x1f);
    write(&mut registers, 4, 2, 2);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    let address = 0xb8000 + (3 * 80 + 4) * 2;
    assert_eq!(&memory[address..address + 4], b"X\x1fX\x1f");
    assert_eq!(load(&memory, 0x450, 2), Ok(0x0304));
    write(&mut registers, 0, 2, 0x0a59);
    write(&mut registers, 4, 2, 1);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    write(&mut registers, 0, 2, 0x0800);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    assert_eq!(read(&registers, 0, 2), 0x1f59);
    write(&mut registers, 0, 2, 0x0f00);
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    assert_eq!(read(&registers, 0, 2), 0x5003);
    assert_eq!(read(&registers, 13, 1), 0);
    write(&mut registers, 0, 2, 0x0004);
    assert_eq!(
        host_interrupt(0x10, &mut registers, &mut memory),
        Err(Trap::UnsupportedInterrupt)
    );
}

#[test]
fn video_teletype_wraps_scrolls_and_preserves_the_callers_registers() {
    reset_dos();
    let mut memory = vec![0; 0xc0000];
    let mut registers = [0; 8192];
    initialize_text_video(&mut memory).unwrap();
    store(&mut memory, 0x450, 2, 0x184f).unwrap();
    write(&mut registers, 0, 2, 0x0e58);
    let expected = registers;
    host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    assert_eq!(load(&memory, 0x450, 2), Ok(0x1800));
    assert_eq!(load(&memory, 0xb8000 + (23 * 80 + 79) * 2, 2), Ok(0x0758));
    assert_eq!(load(&memory, 0xb8000 + 24 * 160, 2), Ok(0x0720));
    assert_eq!(registers, expected);
    for byte in b"A\x08B\r\n" {
        write(&mut registers, 0, 2, 0x0e00 | *byte as u64);
        host_interrupt(0x10, &mut registers, &mut memory).unwrap();
    }
    assert_eq!(get_console_output(), b"XA\x08B\r\n");
    assert_eq!(load(&memory, 0xb8000 + 23 * 160, 2), Ok(0x0742));
    assert_eq!(load(&memory, 0x450, 2), Ok(0x1800));
}

#[test]
fn direct_console_poll_returns_queued_bytes_and_yields_only_when_configured() {
    reset_dos();
    let mut registers = [0; 8192];
    write(&mut registers, 0, 2, 0x0600);
    write(&mut registers, 8, 2, 0xff);
    assert_eq!(host_interrupt(0x21, &mut registers, &mut []), Ok(0));
    assert_eq!(read(&registers, 0, 1), 0);
    assert_eq!(read(&registers, 518, 1), 1);
    queue_input_bytes(b"0\r");
    for byte in b"0\r" {
        write(&mut registers, 0, 2, 0x0600);
        host_interrupt(0x21, &mut registers, &mut []).unwrap();
        assert_eq!(read(&registers, 0, 1), *byte as u64);
        assert_eq!(read(&registers, 518, 1), 0);
    }
    DOS_ENV.with(|env| env.borrow_mut().yield_on_empty_input = true);
    write(&mut registers, 0, 2, 0x0600);
    assert_eq!(
        host_interrupt(0x21, &mut registers, &mut []),
        Err(Trap::InputRequired)
    );
    assert_eq!(read(&registers, 0, 1), 0);
    assert_eq!(read(&registers, 518, 1), 1);
    assert!(get_console_output().is_empty());
}
