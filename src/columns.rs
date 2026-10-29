//! Record columns: the time and the subgroup of every record, compressed.
//!
//! The comparison and spike detection must know each record's time and which row of the pattern
//! table it falls into. That is two numbers per record; stored plainly they would take 12 bytes
//! (an 8-byte time, a 4-byte subgroup number). The columns write them as variable-length
//! integers (LEB128):
//!
//! - The time is written as the difference from the previous record's time (delta encoding).
//!   Records are mostly written in time order; the difference is usually a few seconds and fits
//!   in a single byte. It can also be negative (two processes writing at the same moment);
//!   zigzag encoding turns small negative numbers into small positive numbers too:
//!   0 → 0, −1 → 1, 1 → 2, −2 → 3...
//! - The subgroup number is written as it is: the first 128 subgroups take a single byte.
//!
//! In LEB128 each byte carries 7 bits of the number; the top bit means "more follows". 300 →
//! `0b1_0101100 0b0_0000010`: first the low 7 bits (44), then the rest (2 × 128).
//!
//! A record thus mostly takes 2 or 3 bytes. The columns can only be read from start to end; the
//! comparison and spike detection both go through the records once, from start to end, anyway.

/// The times (seconds since 1970) and subgroup numbers of the records, in the order they were
/// added.
#[derive(Debug, Default, Clone)]
pub struct RecordColumns {
    bytes: Vec<u8>,
    count: usize,
    /// The time of the last record added: the next one is written as a difference from it.
    last_time: i64,
}

impl RecordColumns {
    /// Appends a record.
    pub fn push(&mut self, time: i64, subgroup: u32) {
        // `wrapping_sub`: no panic on overflow. Record times have four-digit years (±3·10¹¹ s),
        // so there is no overflow; even if there were, `wrapping_add` gives the same number back
        // when reading.
        write_leb128(
            &mut self.bytes,
            zigzag_encode(time.wrapping_sub(self.last_time)),
        );
        write_leb128(&mut self.bytes, u64::from(subgroup));
        self.last_time = time;
        self.count += 1;
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Bytes the columns take in memory.
    #[cfg(test)]
    pub fn byte_count(&self) -> usize {
        self.bytes.len()
    }

    /// The records in the order they were added: (time, subgroup).
    pub fn iter(&self) -> ColumnsIter<'_> {
        ColumnsIter {
            bytes: &self.bytes,
            pos: 0,
            time: 0,
            remaining: self.count,
        }
    }
}

/// An iterator that reads the columns from start to end.
pub struct ColumnsIter<'a> {
    bytes: &'a [u8],
    pos: usize,
    time: i64,
    remaining: usize,
}

impl Iterator for ColumnsIter<'_> {
    type Item = (i64, u32);

    fn next(&mut self) -> Option<(i64, u32)> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        self.time = self
            .time
            .wrapping_add(zigzag_decode(read_leb128(self.bytes, &mut self.pos)));
        let subgroup = u32::try_from(read_leb128(self.bytes, &mut self.pos))
            .expect("the subgroup number was written as a u32");
        Some((self.time, subgroup))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

/// Because `size_hint` is exact: `len()` gives the number of records left.
impl ExactSizeIterator for ColumnsIter<'_> {}

/// Zigzag: turns a signed number into an unsigned number that stays small while the absolute
/// value is small. `n >> 63` is a mask whose bits are all 1 for a negative number and all 0 for
/// a positive one.
fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// Writes the number as LEB128: starting from the low 7 bits, 7 bits per byte.
fn write_leb128(bytes: &mut Vec<u8>, mut n: u64) {
    while n >= 0x80 {
        bytes.push((n as u8) | 0x80);
        n >>= 7;
    }
    bytes.push(n as u8);
}

/// Reads the LEB128 number at `pos` and moves `pos` past it. The bytes are as `write_leb128`
/// wrote them.
fn read_leb128(bytes: &[u8], pos: &mut usize) -> u64 {
    let mut n = 0;
    let mut shift = 0;
    loop {
        let byte = bytes[*pos];
        *pos += 1;
        n |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return n;
        }
        shift += 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_keeps_small_numbers_small() {
        let pairs = [
            (0, 0),
            (-1, 1),
            (1, 2),
            (-2, 3),
            (2, 4),
            (63, 126),
            (-64, 127),
        ];
        for (n, code) in pairs {
            assert_eq!(zigzag_encode(n), code, "{n}");
            assert_eq!(zigzag_decode(code), n, "{code}");
        }
        for n in [i64::MIN, i64::MAX, i64::MIN + 1, -1_000_000_007, 1 << 40] {
            assert_eq!(zigzag_decode(zigzag_encode(n)), n, "{n}");
        }
    }

    #[test]
    fn leb128_byte_counts() {
        for (n, expected) in [
            (0, vec![0]),
            (1, vec![1]),
            (127, vec![0x7f]),
            (128, vec![0x80, 1]),
            (300, vec![0b1010_1100, 0b10]),
            (16_383, vec![0xff, 0x7f]),
            (16_384, vec![0x80, 0x80, 1]),
        ] {
            let mut bytes = Vec::new();
            write_leb128(&mut bytes, n);
            assert_eq!(bytes, expected, "{n}");
            let mut pos = 0;
            assert_eq!(read_leb128(&bytes, &mut pos), n);
            assert_eq!(pos, bytes.len());
        }
        // The largest number takes 10 bytes
        let mut bytes = Vec::new();
        write_leb128(&mut bytes, u64::MAX);
        assert_eq!(bytes.len(), 10);
        assert_eq!(read_leb128(&bytes, &mut 0), u64::MAX);
    }

    #[test]
    fn records_read_back_as_added() {
        // In order, going back, in the same second and far apart; small and large subgroups
        let mut records = vec![
            (1_761_300_000, 0),
            (1_761_300_000, 1),
            (1_761_300_007, 0),
            (1_761_299_990, 127),
            (1_761_299_991, 128),
            (-62_167_219_200, 5),
            (253_402_300_799, u32::MAX),
            (0, 3),
        ];
        // Thousands more pseudo-random records (xorshift), to exercise the edge cases
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..5_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let time = 1_761_300_000 + (state % 7_200) as i64 - 3_600;
            records.push((time, (state >> 40) as u32 % 3_000));
        }
        let mut columns = RecordColumns::default();
        for &(time, subgroup) in &records {
            columns.push(time, subgroup);
        }
        assert_eq!(columns.len(), records.len());
        assert_eq!(columns.iter().len(), records.len());
        assert_eq!(columns.iter().collect::<Vec<_>>(), records);
    }

    #[test]
    fn two_bytes_per_record_in_an_ordered_log() {
        // A few records per second, 100 subgroups: the time difference and the subgroup take a
        // byte each
        let mut columns = RecordColumns::default();
        for i in 0..10_000_u32 {
            columns.push(1_761_300_000 + i64::from(i / 3), i % 100);
        }
        // The first record's time is a difference from 1970: 5 bytes
        assert_eq!(columns.byte_count(), 2 * 10_000 + 4);
    }

    #[test]
    fn empty_columns() {
        let columns = RecordColumns::default();
        assert_eq!(columns.len(), 0);
        assert_eq!(columns.iter().next(), None);
    }
}
