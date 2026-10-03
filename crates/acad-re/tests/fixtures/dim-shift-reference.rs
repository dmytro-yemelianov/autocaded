// Independent instruction-level reference: never calls the Pcode/IR evaluator.
fn put16(bank: &mut [u8], offset: usize, value: u16) {
    bank[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn reference_case(bits: u32, count: u16, flags: u8, sp: u16, cs: u16, ss: u16) {
    let mut actual = [0xa5; 8192];
    put16(&mut actual, 0, bits as u16);
    put16(&mut actual, 8, (bits >> 16) as u16);
    put16(&mut actual, 4, count);
    put16(&mut actual, 16, sp);
    put16(&mut actual, 258, cs);
    put16(&mut actual, 260, ss);
    for (bit, offset) in [512, 514, 518, 519, 523].into_iter().enumerate() {
        actual[offset] = (flags >> bit) & 1;
    }
    let mut expected = actual;
    let mut result = bits;
    for _ in 0..count.min(32) {
        let carry = (result >> 31) as u8;
        result = result.wrapping_shl(1);
        // SHL of AX supplies SF/ZF/PF; RCL of DX supplies CF/OF.
        expected[512] = carry;
        expected[514] = u8::from((result as u8).count_ones() % 2 == 0);
        expected[518] = u8::from(result as u16 == 0);
        expected[519] = ((result >> 15) & 1) as u8;
        expected[523] = carry ^ (result >> 31) as u8;
    }
    put16(&mut expected, 0, result as u16);
    put16(&mut expected, 8, (result >> 16) as u16);
    put16(&mut expected, 4, 0);
    put16(&mut expected, 16, sp.wrapping_add(2));
    let return_ip = 0xbeefu16;
    let target = ((cs as u32) << 4) + return_ip as u32;
    expected[644..648].copy_from_slice(&target.to_le_bytes());
    let stack = ((ss as usize) << 4) + sp as usize;
    let mut memory = vec![0; stack + 2];
    put16(&mut memory, stack, return_ip);
    let returned = run(&mut actual, &mut memory, 102).unwrap();
    assert_eq!(returned, target as u64);
    assert_eq!(actual, expected, "bits={bits:08x} count={count} flags={flags:02x} sp={sp:04x}");
}

#[test]
fn every_count_and_boundary_patterns() {
    // Covers unsigned clamp, JCXZ, signed CMP flags, and all loop lengths.
    for count in 0..=u16::MAX {
        reference_case(0x8123_abcd, count, 0x1f, 0, 0x2000, 0);
    }
    for bits in [0, 1, 0x8000_0000, 0xffff_ffff, 0x0001_0000, 0x8000, 0x7fff_ffff, 0xaaaa_5555] {
        for count in 0..=34 {
            for flags in 0..32 {
                reference_case(bits, count, flags, 0xffff, 0xffff, 0x1234);
            }
        }
    }
    let mut seed = 0x7135_924bu32;
    for i in 0..2048 {
        seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
        reference_case(seed, (i % 35) as u16, (i % 32) as u8, 0xfffe, 0x1010, 0);
    }
}

#[test]
fn bounds_and_budget_are_explicit() {
    let mut registers = [0; 8192];
    assert_eq!(run(&mut registers, &mut [], 0), Err(Trap::Budget));
    assert_eq!(run(&mut registers, &mut [], 1), Err(Trap::Budget));
    assert_eq!(run(&mut registers, &mut [], 2), Err(Trap::Memory));
    put16(&mut registers, 260, 0xffff);
    put16(&mut registers, 16, 0xffff);
    assert_eq!(run(&mut registers, &mut [0; 2], 2), Err(Trap::Memory));
}

#[test]
fn arithmetic_runtime_edges() {
    // Shift counts must not silently wrap on conversion to u32.
    assert_eq!(shift_left(1, 64), 0);
    assert_eq!(shift_left(1, 0x1_0000_0001), 0);
    assert_eq!(shift_right(u64::MAX, 0x1_0000_0001), 0);
    assert_eq!(signed(0x8000, 16), -32768);
    assert_eq!(signed(u64::MAX, 64), -1);
}
