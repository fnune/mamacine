//! Decoding yEnc, the encoding every usenet binary arrives in. Forty lines, versus a dependency.

/// The bytes between `=ybegin` and `=yend`, decoded. Anything malformed decodes to what it can:
/// the caller treats the result as evidence, never as the file itself.
pub fn decode(article: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(article.len());
    let mut inside = false;
    for line in article.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b"=ybegin") {
            inside = true;
            continue;
        }
        if line.starts_with(b"=ypart") {
            continue;
        }
        if line.starts_with(b"=yend") {
            break;
        }
        if !inside {
            continue;
        }
        // NNTP dot-stuffing: a line starting ".." carried a line starting "."
        let line = if line.starts_with(b"..") {
            &line[1..]
        } else {
            line
        };
        let mut escaped = false;
        for byte in line {
            if escaped {
                out.push(byte.wrapping_sub(64).wrapping_sub(42));
                escaped = false;
            } else if *byte == b'=' {
                escaped = true;
            } else {
                out.push(byte.wrapping_sub(42));
            }
        }
    }
    out
}

/// The inverse, for tests: real encoders escape more eagerly, decoders must not care.
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = b"=ybegin line=128 size=0 name=test\r\n".to_vec();
    for byte in data {
        let coded = byte.wrapping_add(42);
        if matches!(coded, 0 | b'\r' | b'\n' | b'=' | b'.') {
            out.push(b'=');
            out.push(coded.wrapping_add(64));
        } else {
            out.push(coded);
        }
    }
    out.extend(b"\r\n=yend size=0\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_encoded_decodes_back_byte_for_byte() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        assert_eq!(decode(&encode(&data)), data);
    }

    #[test]
    fn garbage_decodes_to_something_rather_than_panicking() {
        assert!(decode(b"not yenc at all").is_empty());
        assert!(decode(b"=ybegin\r\n=yend").is_empty());
    }
}
