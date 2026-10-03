// Independent model of the byte operations and addressing, without Pcode.
fn put16(bank: &mut [u8], offset: usize, value: u16) {
    bank[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn reference_case(word: u16, sp: u16, ds: u16, ss: u16, cs: u16) {
    let mut actual = [0xa5; 8192];
    put16(&mut actual, 16, sp);
    put16(&mut actual, 258, cs);
    put16(&mut actual, 260, ss);
    put16(&mut actual, 262, ds);
    for offset in [512, 514, 518, 519, 523] { actual[offset] = 1; }
    let mut expected = actual;
    put16(&mut expected, 12, sp);
    put16(&mut expected, 16, sp.wrapping_add(2));
    let test_address = ((ds as usize) << 4) + sp.wrapping_add(8) as usize;
    let flip_address = ((ds as usize) << 4) + sp.wrapping_add(9) as usize;
    let stack_address = ((ss as usize) << 4) + sp as usize;
    let mut memory = vec![0x5a; (test_address + 2).max(flip_address + 1).max(stack_address + 2)];
    put16(&mut memory, test_address, word);
    memory[flip_address] = (word >> 8) as u8;
    put16(&mut memory, stack_address, 0xbeef);
    let mut expected_memory = memory.clone();
    let masked = word & 0x7ff0;
    expected[512] = 0;
    expected[523] = 0;
    if masked == 0 {
        expected[514] = 1;
        expected[518] = 1;
        expected[519] = 0;
    } else {
        expected_memory[flip_address] ^= 0x80;
        let byte = expected_memory[flip_address];
        expected[514] = u8::from(byte.count_ones() % 2 == 0);
        expected[518] = u8::from(byte == 0);
        expected[519] = byte >> 7;
    }
    let target = ((cs as u32) << 4) + 0xbeef;
    expected[644..648].copy_from_slice(&target.to_le_bytes());
    assert_eq!(run(&mut actual, &mut memory, 5), Ok(target as u64));
    assert_eq!(actual, expected, "word={word:04x} sp={sp:04x}");
    assert_eq!(memory, expected_memory, "word={word:04x} sp={sp:04x}");
}

#[test]
fn every_high_word_and_segment_boundaries() {
    for word in 0..=u16::MAX { reference_case(word, 0, 0, 0, 0x2000); }
    // DS addresses the argument, SS addresses the return. In the linear RAM
    // model a two-byte LOAD across offset FFFF does not wrap, while the next
    // instruction's SP+9 arithmetic wraps as a 16-bit integer.
    for word in [0, 1, 0xf, 0x10, 0x7ff0, 0x8000, 0x800f, 0xfff0, 0xffff] {
        for sp in [0xfff6, 0xfff7, 0xfff8, 0xfffe, 0xffff] {
            reference_case(word, sp, 0x10, 0x120, 0xffff);
        }
    }
}

#[test]
fn checked_store_and_load_boundaries() {
    let mut bytes = [0x55; 4];
    assert_eq!(store(&mut bytes, 3, 2, 0), Err(Trap::Memory));
    assert_eq!(bytes, [0x55; 4]);
    assert_eq!(store(&mut bytes, u64::MAX, 1, 0), Err(Trap::Memory));
    store(&mut bytes, 1, 2, 0x1234).unwrap();
    assert_eq!(bytes, [0x55, 0x34, 0x12, 0x55]);
    assert_eq!(load(&bytes, 1, 2), Ok(0x1234));
}
