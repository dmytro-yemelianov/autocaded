#[test]
fn selects_cases_and_default_using_ss_and_consumes_one_word() {
    for ss in [0, 0x120, 0x1e25] {
        for sp in [0x40u16, 0xfffe] {
            for (selector, expected) in [(114, 1), (0xffff, 2), (0, 3), (119, 3)] {
                let mut registers = [0xa5; 8192];
                write(&mut registers, 260, 2, ss);
                write(&mut registers, 262, 2, 0x3000); // DS differs from SS.
                write(&mut registers, 16, 2, sp as u64);
                let mut expected_registers = registers;
                write(&mut expected_registers, 0, 2, selector);
                write(&mut expected_registers, 16, 2, sp.wrapping_add(2) as u64);
                let mut memory = vec![0x55; 1024 * 1024];
                // A conflicting value at the flat SP address exposes the old bug.
                store(&mut memory, sp as u64, 2, 119).unwrap();
                store(&mut memory, (0x3000 << 4) + sp as u64, 2, 119).unwrap();
                store(&mut memory, (ss << 4) + sp as u64, 2, selector).unwrap();
                let expected_memory = memory.clone();
                assert_eq!(run(&mut registers, &mut memory, 2), Ok(expected));
                assert_eq!(registers, expected_registers);
                assert_eq!(memory, expected_memory);
            }
        }
    }
}

#[test]
fn rejects_out_of_bounds_stack_without_consuming_the_value() {
    let mut registers = [0; 8192];
    write(&mut registers, 260, 2, 0x100);
    write(&mut registers, 16, 2, 0x20);
    let expected_registers = registers;
    let mut memory = vec![0; 0x1021]; // Only one byte at SS:SP.
    assert_eq!(run(&mut registers, &mut memory, 2), Err(Trap::Memory));
    assert_eq!(registers, expected_registers);
}
