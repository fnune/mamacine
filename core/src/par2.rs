//! Reading a par2 index file's table of contents: which files the repair set can actually
//! repair. The Joy season died with all its repair data present, because the damage sat in
//! files the set does not cover — a fact this small file states outright, before any download.

/// What one par2 index promises to protect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Protection {
    /// Protected file names, lowercased.
    pub covered: Vec<String>,
    pub block_size: u64,
}

const MAGIC: &[u8; 8] = b"PAR2\0PKT";

/// Whether these bytes are par2 at all. Posts disguise data as ".par2" to dodge scanners — the
/// Joy season shipped ten such fakes, nzbget found "nothing to par-check", and any damage was
/// fatal. One fetched article answers this before a byte of the release is downloaded.
pub fn contains_packets(bytes: &[u8]) -> bool {
    bytes.windows(MAGIC.len()).any(|window| window == MAGIC)
}

/// Reads whatever packets are present; a truncated tail packet is simply ignored, because the
/// caller may only hold the first article of the file.
pub fn read(bytes: &[u8]) -> Option<Protection> {
    let mut protection = Protection::default();
    let mut at = 0usize;
    let mut found = false;
    while at + 64 <= bytes.len() {
        if &bytes[at..at + 8] != MAGIC {
            at += 1;
            continue;
        }
        let length = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().ok()?) as usize;
        if length < 64 {
            at += 8;
            continue;
        }
        let packet_type = &bytes[at + 48..at + 64];
        let body_end = (at + length).min(bytes.len());
        let body = &bytes[at + 64..body_end];
        let whole = at + length <= bytes.len();

        if packet_type.starts_with(b"PAR 2.0\0Main") && whole && body.len() >= 8 {
            protection.block_size = u64::from_le_bytes(body[0..8].try_into().ok()?);
            found = true;
        }
        if packet_type.starts_with(b"PAR 2.0\0FileDesc") && whole && body.len() > 56 {
            // file id, two hashes and a length come first; the name fills the rest, padded
            let name: Vec<u8> = body[56..]
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .collect();
            if let Ok(name) = String::from_utf8(name) {
                protection.covered.push(name.to_lowercase());
                found = true;
            }
        }
        at += length.max(8);
    }
    found.then_some(protection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let length = 64 + body.len();
        let mut out = MAGIC.to_vec();
        out.extend((length as u64).to_le_bytes());
        out.extend([0u8; 16]); // packet hash, unchecked: this is evidence, not verification
        out.extend([0u8; 16]); // set id
        let mut named = [0u8; 16];
        named[..kind.len()].copy_from_slice(kind);
        out.extend(named);
        out.extend(body);
        out
    }

    fn file_desc(name: &str) -> Vec<u8> {
        let mut body = vec![0u8; 56];
        body.extend(name.as_bytes());
        body.extend([0u8; 4]); // par2 pads names to multiples of four
        packet(b"PAR 2.0\0FileDesc", &body)
    }

    #[test]
    fn the_index_names_what_the_repair_set_covers() {
        let mut index = packet(b"PAR 2.0\0Main", &{
            let mut body = 384_000u64.to_le_bytes().to_vec();
            body.extend(2u32.to_le_bytes());
            body
        });
        index.extend(file_desc("Show.S01E01.mkv"));
        index.extend(file_desc("Show.S01E02.MKV"));

        let protection = read(&index).expect("readable");
        assert_eq!(protection.block_size, 384_000);
        assert_eq!(protection.covered, ["show.s01e01.mkv", "show.s01e02.mkv"]);
    }

    #[test]
    fn a_truncated_tail_packet_is_ignored_rather_than_fatal() {
        let mut index = file_desc("covered.mkv");
        index.extend(&file_desc("lost.mkv")[..70]); // the article ended mid-packet
        let protection = read(&index).expect("readable");
        assert_eq!(protection.covered, ["covered.mkv"]);
    }

    #[test]
    fn bytes_that_are_not_par2_are_no_answer_at_all() {
        assert_eq!(read(b"definitely not a par2 file"), None);
        assert_eq!(read(&[]), None);
    }

    // A recovery volume can open with a packet larger than the fetched article, hiding the
    // vital packets beyond it: authenticity needs only the magic, not a whole packet.
    #[test]
    fn real_par2_is_recognised_even_when_only_a_giant_packet_head_was_fetched() {
        let mut giant = MAGIC.to_vec();
        giant.extend(2_000_000u64.to_le_bytes());
        giant.extend([0u8; 100]); // the article ended long before the packet did
        assert!(contains_packets(&giant));
        assert!(!contains_packets(b"data wearing a .par2 name"));
    }
}
