//! Reading an MP4 header without a probe program.

use crate::media::MediaInfo;

const UNSPECIFIED: &str = "und";

/// `moov` sits at either end; offer both.
pub fn read_header(front: &[u8], tail: &[u8]) -> MediaInfo {
    let mut info = MediaInfo::default();
    let Some(moov) = find_moov(front).or_else(|| find_moov(tail)) else {
        return info;
    };

    for (name, body) in Boxes::over(moov) {
        match &name {
            b"mvhd" => info.duration_seconds = movie_duration(body),
            b"trak" => read_track(body, &mut info),
            _ => {}
        }
    }
    info
}

fn find_moov(slice: &[u8]) -> Option<&[u8]> {
    for (at, window) in slice.windows(4).enumerate().skip(4) {
        if window != b"moov" {
            continue;
        }
        let start = at - 4;
        let Some(size) = u32_at(slice, start) else {
            continue;
        };
        let size = size as usize;
        if size >= 8 && size <= slice.len() - start {
            return Some(&slice[at + 4..start + size]);
        }
    }
    None
}

fn read_track(trak: &[u8], info: &mut MediaInfo) {
    let Some(mdia) = child(trak, b"mdia") else {
        return;
    };

    let mut handler = None;
    let mut language = None;
    let mut timescale = None;
    let mut sample_times = None;
    for (name, body) in Boxes::over(mdia) {
        match &name {
            b"mdhd" => {
                language = language_of(body);
                timescale = timescale_and_duration(body).map(|(timescale, _)| timescale);
            }
            b"hdlr" => handler = fourcc_at(body, 8),
            b"minf" => sample_times = child(body, b"stbl").and_then(|stbl| child(stbl, b"stts")),
            _ => {}
        }
    }

    let language = language.unwrap_or_else(|| UNSPECIFIED.to_string());
    let Some(handler) = handler else {
        return;
    };
    match &handler {
        b"vide" => {
            if let Some(fps) = sample_times.and_then(|stts| frame_rate(stts, timescale)) {
                info.fps = Some(fps);
            }
        }
        b"soun" => push(&mut info.audio_languages, language),
        b"text" | b"sbtl" | b"subt" => push(&mut info.subtitle_languages, language),
        _ => {}
    }
}

fn movie_duration(mvhd: &[u8]) -> Option<f64> {
    let (timescale, ticks) = timescale_and_duration(mvhd)?;
    if timescale == 0 || ticks == u64::from(u32::MAX) || ticks == u64::MAX {
        return None;
    }
    Some(ticks as f64 / f64::from(timescale))
}

fn timescale_and_duration(body: &[u8]) -> Option<(u32, u64)> {
    match body.first()? {
        0 => Some((u32_at(body, 12)?, u64::from(u32_at(body, 16)?))),
        1 => Some((u32_at(body, 20)?, u64_at(body, 24)?)),
        _ => None,
    }
}

fn language_of(mdhd: &[u8]) -> Option<String> {
    let at = match mdhd.first()? {
        0 => 20,
        1 => 32,
        _ => return None,
    };
    let packed = u16_at(mdhd, at)?;
    let letters = [packed >> 10, packed >> 5, packed].map(|part| ((part & 0x1F) as u8) + 0x60);
    if !letters.iter().all(u8::is_ascii_lowercase) {
        return None;
    }
    String::from_utf8(letters.to_vec()).ok()
}

fn frame_rate(stts: &[u8], timescale: Option<u32>) -> Option<f64> {
    let timescale = timescale.filter(|timescale| *timescale > 0)?;
    let first_delta = u32_at(stts, 12).filter(|delta| *delta > 0)?;
    let fps = f64::from(timescale) / f64::from(first_delta);
    Some((fps * 1000.0).round() / 1000.0)
}

fn child<'a>(parent: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    Boxes::over(parent)
        .find(|(found, _)| found == name)
        .map(|(_, body)| body)
}

fn push(list: &mut Vec<String>, language: String) {
    if !list.contains(&language) {
        list.push(language);
    }
}

struct Boxes<'a> {
    rest: &'a [u8],
}

impl<'a> Boxes<'a> {
    fn over(bytes: &'a [u8]) -> Boxes<'a> {
        Boxes { rest: bytes }
    }
}

impl<'a> Iterator for Boxes<'a> {
    type Item = ([u8; 4], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let declared = u64::from(u32_at(self.rest, 0)?);
        let name = fourcc_at(self.rest, 4)?;

        let (header, size) = match declared {
            0 => (8, self.rest.len() as u64),
            1 => {
                let large = u64_at(self.rest, 8)?;
                if large < 16 {
                    return None;
                }
                (16, large)
            }
            2..=7 => return None,
            _ => (8, declared),
        };

        let end = usize::try_from(size).map_or(self.rest.len(), |size| size.min(self.rest.len()));
        let body = self.rest.get(header..end).unwrap_or(&[]);
        self.rest = self.rest.get(end..).unwrap_or(&[]);
        Some((name, body))
    }
}

fn fourcc_at(bytes: &[u8], at: usize) -> Option<[u8; 4]> {
    bytes.get(at..at.checked_add(4)?)?.try_into().ok()
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes(raw.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes(raw.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let raw = bytes.get(at..at.checked_add(8)?)?;
    Some(u64::from_be_bytes(raw.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend(name);
        out.extend(body);
        out
    }

    fn ftyp() -> Vec<u8> {
        boxed(b"ftyp", b"isom\x00\x00\x02\x00isomiso2")
    }

    fn mvhd(timescale: u32, duration: u32) -> Vec<u8> {
        let mut body = vec![0; 12];
        body.extend(timescale.to_be_bytes());
        body.extend(duration.to_be_bytes());
        boxed(b"mvhd", &body)
    }

    fn mvhd_v1(timescale: u32, duration: u64) -> Vec<u8> {
        let mut body = vec![1];
        body.extend([0; 19]);
        body.extend(timescale.to_be_bytes());
        body.extend(duration.to_be_bytes());
        boxed(b"mvhd", &body)
    }

    fn packed(language: &str) -> u16 {
        language
            .bytes()
            .fold(0, |value, letter| (value << 5) | u16::from(letter - 0x60))
    }

    fn track(
        handler: &[u8; 4],
        language: u16,
        timescale: u32,
        frame_delta: Option<u32>,
    ) -> Vec<u8> {
        let mut mdhd = vec![0; 12];
        mdhd.extend(timescale.to_be_bytes());
        mdhd.extend(0u32.to_be_bytes());
        mdhd.extend(language.to_be_bytes());

        let mut hdlr = vec![0; 8];
        hdlr.extend(handler);

        let mut mdia = boxed(b"mdhd", &mdhd);
        mdia.extend(boxed(b"hdlr", &hdlr));
        if let Some(delta) = frame_delta {
            let mut stts = vec![0; 4];
            stts.extend(1u32.to_be_bytes());
            stts.extend(1000u32.to_be_bytes());
            stts.extend(delta.to_be_bytes());
            mdia.extend(boxed(b"minf", &boxed(b"stbl", &boxed(b"stts", &stts))));
        }
        boxed(b"trak", &boxed(b"mdia", &mdia))
    }

    fn film() -> Vec<u8> {
        let moov = [
            mvhd(1000, 7_654_321),
            track(b"vide", 0, 24_000, Some(1001)),
            track(b"soun", packed("spa"), 48_000, None),
            track(b"soun", packed("eng"), 48_000, None),
            track(b"soun", 0, 48_000, None),
            track(b"text", packed("spa"), 1000, None),
        ]
        .concat();
        [ftyp(), boxed(b"moov", &moov)].concat()
    }

    #[test]
    fn finds_moov_at_the_front_of_the_file() {
        let info = read_header(&film(), &[]);
        assert!(info.duration_seconds.is_some());
        assert!(!info.audio_languages.is_empty());
    }

    #[test]
    fn finds_moov_in_a_tail_that_starts_inside_another_box() {
        let mut middle = vec![0x6D; 10_000];
        middle.splice(
            5_000..5_000,
            [0xFF, 0xFF, 0xFF, 0xFF, b'm', b'o', b'o', b'v'],
        );

        let mut file = ftyp();
        file.extend(boxed(b"mdat", &middle));
        let moov_starts = file.len();
        file.extend(&film()[ftyp().len()..]);

        let front = &file[..64];
        let tail = &file[moov_starts - 4_000..];
        let info = read_header(front, tail);
        assert_eq!(info.audio_languages, vec!["spa", "eng", "und"]);
        assert!(info.duration_seconds.is_some());
    }

    #[test]
    fn reads_audio_languages_and_reports_an_untagged_track_as_unknown() {
        let info = read_header(&film(), &[]);
        assert_eq!(info.audio_languages, vec!["spa", "eng", "und"]);
        assert!(info.has_spanish());
    }

    #[test]
    fn a_text_handler_track_counts_as_subtitles() {
        assert_eq!(read_header(&film(), &[]).subtitle_languages, vec!["spa"]);
    }

    #[test]
    fn a_language_packed_into_fifteen_bits_is_read_back_as_letters() {
        let moov = boxed(b"moov", &track(b"soun", packed("deu"), 48_000, None));
        assert_eq!(read_header(&moov, &[]).audio_languages, vec!["deu"]);
    }

    #[test]
    fn reads_the_frame_rate_from_the_first_sample_delta() {
        let fps = read_header(&film(), &[]).fps.expect("a frame rate");
        assert!((fps - 23.976).abs() < 0.001, "{fps}");
    }

    #[test]
    fn a_zero_sample_delta_yields_no_frame_rate_rather_than_infinity() {
        let moov = boxed(b"moov", &track(b"vide", 0, 24_000, Some(0)));
        assert_eq!(read_header(&moov, &[]).fps, None);
    }

    #[test]
    fn reads_the_duration_in_seconds_from_the_movie_header() {
        let seconds = read_header(&film(), &[])
            .duration_seconds
            .expect("a duration");
        assert!((seconds - 7654.321).abs() < 0.001, "{seconds}");
    }

    #[test]
    fn a_version_one_movie_header_is_read_the_same() {
        let moov = boxed(b"moov", &mvhd_v1(1000, 7_654_321));
        let seconds = read_header(&moov, &[])
            .duration_seconds
            .expect("a duration");
        assert!((seconds - 7654.321).abs() < 0.001, "{seconds}");
    }

    #[test]
    fn garbage_and_emptiness_yield_nothing_rather_than_panicking() {
        assert_eq!(read_header(&[], &[]), MediaInfo::default());
        assert_eq!(
            read_header(b"not a film at all", b"nor is this"),
            MediaInfo::default()
        );
    }

    #[test]
    fn a_truncated_file_yields_what_it_can_rather_than_panicking() {
        let whole = film();
        for cut in [3, 12, 30, 60, whole.len() / 2, whole.len() - 1] {
            let _ = read_header(&whole[..cut], &[]);
            let _ = read_header(&[], &whole[..cut]);
        }
    }

    #[test]
    fn corrupted_sizes_are_skipped_without_panicking_or_looping() {
        let oversized_child = {
            let mut body = vec![0xFF; 4];
            body.extend(b"trak");
            body.extend([0; 40]);
            boxed(b"moov", &body)
        };
        let zero_sized_child_followed_by_junk = {
            let mut body = 0u32.to_be_bytes().to_vec();
            body.extend(b"trak");
            body.extend([0x6D; 40]);
            boxed(b"moov", &body)
        };
        let undersized_child = {
            let mut body = 5u32.to_be_bytes().to_vec();
            body.extend(b"trak");
            body.extend([0; 40]);
            boxed(b"moov", &body)
        };
        let largesize_missing = {
            let mut body = 1u32.to_be_bytes().to_vec();
            body.extend(b"trak");
            boxed(b"moov", &body)
        };
        let largesize_smaller_than_its_own_header = {
            let mut body = 1u32.to_be_bytes().to_vec();
            body.extend(b"trak");
            body.extend(8u64.to_be_bytes());
            body.extend([0; 40]);
            boxed(b"moov", &body)
        };

        for bytes in [
            oversized_child,
            zero_sized_child_followed_by_junk,
            undersized_child,
            largesize_missing,
            largesize_smaller_than_its_own_header,
        ] {
            let _ = read_header(&bytes, &[]);
            let _ = read_header(&[], &bytes);
        }
    }
}
