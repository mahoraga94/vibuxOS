const fn make_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut crc = n as u32;
        let mut k = 0usize;
        while k < 8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0x82F6_3B78u32 & mask);
            k += 1;
        }
        table[n] = crc;
        n += 1;
    }
    table
}

static CRC32C_TABLE: [u32; 256] = make_crc32c_table();

#[inline(always)]
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32C_TABLE[index];
    }
    !crc
}
