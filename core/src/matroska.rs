//! Reading what a Matroska file says about itself, from its own header.
//!
//! This exists so the app does not have to carry ffmpeg. It needs two things from a finished film:
//! how long it runs, and what languages its tracks are in. Both sit in the first megabyte, in a
//! format simple enough to read directly.

use crate::media::MediaInfo;

// The elements worth reading, by their EBML identifier.
const SEGMENT: u64 = 0x1853_8067;
const INFO: u64 = 0x1549_A966;
const TIMECODE_SCALE: u64 = 0x002A_D7B1;
const DURATION: u64 = 0x4489;
const TRACKS: u64 = 0x1654_AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_TYPE: u64 = 0x83;
const LANGUAGE: u64 = 0x0022_B59C;
const LANGUAGE_BCP47: u64 = 0x0022_B59D;
const DEFAULT_DURATION: u64 = 0x0023_E383;

const TRACK_VIDEO: u64 = 1;
const TRACK_AUDIO: u64 = 2;
const TRACK_SUBTITLE: u64 = 17;

/// The specification says a track without a language element is English. In practice a release
/// omits the tag exactly when nobody set one, and a Chilean film whose audio track says nothing is
/// not English. Reporting it as unknown is the honest answer; reporting it as English is a claim.
const UNSPECIFIED: &str = "und";

pub fn looks_like_matroska(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
}

/// Reads as much as the given bytes allow. The caller passes the front of the file, not all of it.
pub fn read_header(bytes: &[u8]) -> MediaInfo {
    let mut info = MediaInfo::default();
    if !looks_like_matroska(bytes) {
        return info;
    }

    let mut scale = 1_000_000.0; // nanoseconds per tick, and Matroska's default
    let mut ticks = None;

    for (id, body) in Elements::over(bytes) {
        if id != SEGMENT {
            continue;
        }
        for (id, body) in Elements::over(body) {
            match id {
                INFO => {
                    for (id, body) in Elements::over(body) {
                        match id {
                            TIMECODE_SCALE => scale = unsigned(body) as f64,
                            DURATION => ticks = float(body),
                            _ => {}
                        }
                    }
                }
                TRACKS => {
                    for (id, body) in Elements::over(body) {
                        if id == TRACK_ENTRY {
                            read_track(body, &mut info);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    info.duration_seconds = ticks.map(|ticks| ticks * scale / 1_000_000_000.0);
    info
}

fn read_track(body: &[u8], info: &mut MediaInfo) {
    let mut kind = 0;
    let mut language = None;
    let mut frame_nanoseconds = None;

    for (id, body) in Elements::over(body) {
        match id {
            TRACK_TYPE => kind = unsigned(body),
            // the BCP 47 form wins where both are present, as the specification says
            LANGUAGE if language.is_none() => language = text(body),
            LANGUAGE_BCP47 => language = text(body),
            DEFAULT_DURATION => frame_nanoseconds = Some(unsigned(body)),
            _ => {}
        }
    }

    let language = language.unwrap_or_else(|| UNSPECIFIED.to_string());
    match kind {
        TRACK_VIDEO => {
            if let Some(nanoseconds) = frame_nanoseconds.filter(|value| *value > 0) {
                let fps = 1_000_000_000.0 / nanoseconds as f64;
                info.fps = Some((fps * 1000.0).round() / 1000.0);
            }
        }
        TRACK_AUDIO => push(&mut info.audio_languages, language),
        TRACK_SUBTITLE => push(&mut info.subtitle_languages, language),
        _ => {}
    }
}

fn push(list: &mut Vec<String>, language: String) {
    if !list.contains(&language) {
        list.push(language);
    }
}

/// Walks the elements of one level, stopping at the first thing it cannot read.
struct Elements<'a> {
    rest: &'a [u8],
}

impl<'a> Elements<'a> {
    fn over(bytes: &'a [u8]) -> Elements<'a> {
        Elements { rest: bytes }
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = (u64, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let (id, after_id) = variable_width(self.rest, true)?;
        let (size, after_size) = variable_width(after_id, false)?;

        // an unknown size means the element runs to the end of what we were given
        let length = if size == u64::MAX {
            after_size.len()
        } else {
            (size as usize).min(after_size.len())
        };
        let (body, rest) = after_size.split_at(length);
        self.rest = rest;
        Some((id, body))
    }
}

/// EBML numbers are prefixed by the count of leading zero bits. Identifiers keep that marker;
/// everything else strips it.
fn variable_width(bytes: &[u8], keep_marker: bool) -> Option<(u64, &[u8])> {
    let first = *bytes.first()?;
    if first == 0 {
        return None; // more than eight bytes wide, which nothing here uses
    }
    let width = first.leading_zeros() as usize + 1;
    if bytes.len() < width {
        return None;
    }

    // at the widest, the marker uses the whole first byte and every data bit is in the ones after
    let data_bits = if width >= 8 { 0 } else { 0xFF_u8 >> width };
    let mut value = if keep_marker {
        u64::from(first)
    } else {
        u64::from(first & data_bits)
    };
    for byte in &bytes[1..width] {
        value = (value << 8) | u64::from(*byte);
    }

    // all data bits set is Matroska's "size unknown"
    if !keep_marker && value == (1u64 << (7 * width)) - 1 {
        value = u64::MAX;
    }
    Some((value, &bytes[width..]))
}

fn unsigned(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
}

fn float(bytes: &[u8]) -> Option<f64> {
    match bytes.len() {
        4 => Some(f32::from_be_bytes(bytes.try_into().ok()?) as f64),
        8 => Some(f64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

fn text(bytes: &[u8]) -> Option<String> {
    let text: String = String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string();
    Some(text).filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds elements the way a muxer would, so the tests read the real format rather than a mock.
    fn element(id: u64, body: &[u8]) -> Vec<u8> {
        let mut out = id_bytes(id);
        out.extend(size_bytes(body.len() as u64));
        out.extend(body);
        out
    }

    fn id_bytes(id: u64) -> Vec<u8> {
        let width = match id {
            0..=0xFF => 1,
            0x100..=0xFFFF => 2,
            0x1_0000..=0xFF_FFFF => 3,
            _ => 4,
        };
        id.to_be_bytes()[8 - width..].to_vec()
    }

    fn size_bytes(size: u64) -> Vec<u8> {
        if size < 0x7F {
            vec![0x80 | size as u8]
        } else {
            let mut out = vec![0x08];
            out.extend(&size.to_be_bytes()[1..]);
            out
        }
    }

    fn film() -> Vec<u8> {
        let info = element(
            INFO,
            &[
                element(TIMECODE_SCALE, &[0x0F, 0x42, 0x40]).as_slice(), // a million nanoseconds
                element(DURATION, &12_488_544.0f64.to_be_bytes()).as_slice(),
            ]
            .concat(),
        );

        let video = element(
            TRACK_ENTRY,
            &[
                element(TRACK_TYPE, &[TRACK_VIDEO as u8]).as_slice(),
                element(DEFAULT_DURATION, &41_708_333u64.to_be_bytes()[3..]).as_slice(),
            ]
            .concat(),
        );
        let german = element(
            TRACK_ENTRY,
            &[
                element(TRACK_TYPE, &[TRACK_AUDIO as u8]).as_slice(),
                element(LANGUAGE, b"ger").as_slice(),
            ]
            .concat(),
        );
        let english = element(
            TRACK_ENTRY,
            &[
                element(TRACK_TYPE, &[TRACK_AUDIO as u8]).as_slice(),
                element(LANGUAGE, b"eng").as_slice(),
            ]
            .concat(),
        );
        let subtitles = element(
            TRACK_ENTRY,
            &[
                element(TRACK_TYPE, &[TRACK_SUBTITLE as u8]).as_slice(),
                element(LANGUAGE, b"spa").as_slice(),
            ]
            .concat(),
        );

        let tracks = element(TRACKS, &[video, german, english, subtitles].concat());
        let segment = element(SEGMENT, &[info, tracks].concat());

        let mut file = vec![0x1A, 0x45, 0xDF, 0xA3, 0x84, 0x00, 0x00, 0x00, 0x00];
        file.extend(segment);
        file
    }

    #[test]
    fn reads_the_length_of_the_film() {
        let info = read_header(&film());
        let seconds = info.duration_seconds.expect("a duration");
        assert!((seconds - 12488.544).abs() < 0.001, "{seconds}");
    }

    #[test]
    fn reads_the_frame_rate_from_the_video_track() {
        assert_eq!(read_header(&film()).fps, Some(23.976));
    }

    #[test]
    fn reads_every_track_language_once() {
        let info = read_header(&film());
        assert_eq!(info.audio_languages, vec!["ger", "eng"]);
        assert_eq!(info.subtitle_languages, vec!["spa"]);
        assert!(info.has_spanish());
    }

    /// The specification would call this English. Saying so about a film that merely forgot to tag
    /// its audio is a false claim, and the interface is built on never making one.
    #[test]
    fn a_track_that_does_not_say_is_unknown_rather_than_english() {
        let track = element(TRACK_ENTRY, &element(TRACK_TYPE, &[TRACK_AUDIO as u8]));
        let mut file = vec![0x1A, 0x45, 0xDF, 0xA3, 0x84, 0x00, 0x00, 0x00, 0x00];
        file.extend(element(SEGMENT, &element(TRACKS, &track)));
        let info = read_header(&file);
        assert_eq!(info.audio_languages, vec!["und"]);
        assert!(info.language_is_unknown());
        assert!(!info.has_spanish());
    }

    /// A film of any size writes its Segment length across eight bytes. Every real file does this,
    /// and reading one used to overflow a shift and take the whole finishing thread down with it.
    #[test]
    fn reads_a_length_written_across_eight_bytes() {
        let mut file = vec![0x1A, 0x45, 0xDF, 0xA3, 0x84, 0x00, 0x00, 0x00, 0x00];
        file.extend(id_bytes(SEGMENT));
        // 0x01 marks eight bytes wide, and the seven that follow carry the length
        file.push(0x01);
        let body = element(
            TRACKS,
            &element(
                TRACK_ENTRY,
                &[
                    element(TRACK_TYPE, &[TRACK_AUDIO as u8]).as_slice(),
                    element(LANGUAGE, b"spa").as_slice(),
                ]
                .concat(),
            ),
        );
        file.extend(&(body.len() as u64).to_be_bytes()[1..]);
        file.extend(&body);

        let info = read_header(&file);
        assert_eq!(info.audio_languages, vec!["spa"]);
    }

    #[test]
    fn a_length_of_unknown_size_is_read_to_the_end() {
        let mut file = vec![0x1A, 0x45, 0xDF, 0xA3, 0x84, 0x00, 0x00, 0x00, 0x00];
        file.extend(id_bytes(SEGMENT));
        file.push(0xFF); // one byte wide, every data bit set: size unknown
        file.extend(element(
            TRACKS,
            &element(
                TRACK_ENTRY,
                &[
                    element(TRACK_TYPE, &[TRACK_SUBTITLE as u8]).as_slice(),
                    element(LANGUAGE, b"eng").as_slice(),
                ]
                .concat(),
            ),
        ));

        assert_eq!(read_header(&file).subtitle_languages, vec!["eng"]);
    }

    #[test]
    fn something_that_is_not_matroska_yields_nothing() {
        assert_eq!(read_header(b"not a film at all"), MediaInfo::default());
        assert_eq!(read_header(&[]), MediaInfo::default());
    }

    #[test]
    fn a_truncated_file_yields_what_it_can_rather_than_panicking() {
        let whole = film();
        for cut in [12, 30, 60, whole.len() / 2, whole.len() - 1] {
            let _ = read_header(&whole[..cut]);
        }
    }
}
