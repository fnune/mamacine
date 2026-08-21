//! Whether a download will fit, decided before it starts rather than discovered at ninety percent.

const GIGABYTE: u64 = 1_073_741_824;

/// nzbget holds the archive and the unpacked film at once, so a download needs roughly twice its
/// own size while it works, and a little more for the par2 files beside it.
pub const WORKING_MULTIPLE: f64 = 2.2;

/// What is left alone. A disk with nothing free stops being a computer she can use.
pub const RESERVE: u64 = 5 * GIGABYTE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// It fits with room to spare.
    Fits,
    /// It fits, but the disk will be close to full afterwards.
    Tight,
    /// It does not fit, and starting it would only waste her evening.
    NotEnough,
}

pub fn needed_for(download_bytes: u64) -> u64 {
    (download_bytes as f64 * WORKING_MULTIPLE) as u64
}

pub fn room_for(free_bytes: u64, download_bytes: u64) -> Room {
    let needed = needed_for(download_bytes);
    if free_bytes < needed.saturating_add(RESERVE) {
        // the reserve is not a nicety: unpacking into a full disk fails halfway and leaves a mess
        if free_bytes < needed {
            return Room::NotEnough;
        }
        return Room::Tight;
    }
    Room::Fits
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILM: u64 = 2 * GIGABYTE;

    #[test]
    fn a_normal_film_on_a_healthy_disk_fits() {
        assert_eq!(room_for(120 * GIGABYTE, FILM), Room::Fits);
    }

    #[test]
    fn counts_the_room_unpacking_needs_rather_than_the_download_alone() {
        // four and a half gigabytes free, for a two gigabyte film that needs about four and a half
        assert_eq!(room_for(4 * GIGABYTE + GIGABYTE / 2, FILM), Room::Tight);
        assert_eq!(room_for(3 * GIGABYTE, FILM), Room::NotEnough);
    }

    #[test]
    fn leaves_her_disk_something_to_live_on() {
        // it would fit, but only by filling the disk to the brim
        assert_eq!(room_for(needed_for(FILM) + GIGABYTE, FILM), Room::Tight);
        assert_eq!(room_for(needed_for(FILM) + RESERVE, FILM), Room::Fits);
    }

    #[test]
    fn a_film_larger_than_the_disk_is_refused() {
        assert_eq!(room_for(20 * GIGABYTE, 40 * GIGABYTE), Room::NotEnough);
    }
}
