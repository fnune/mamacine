//! Everything Mamá Cine knows how to do, with no user interface and no ambient configuration.
//!
//! Nothing in this crate reads an environment variable, a file or a keyring on its own. Values
//! arrive as arguments; side effects arrive as traits the caller implements.

pub mod clock;
pub mod error;
pub mod films;
pub mod http;
pub mod identity;
pub mod indexer;
pub mod lookup;
pub mod matroska;
pub mod media;
pub mod mp4;
pub mod net;
pub mod nntp;
pub mod nzb;
pub mod nzbget;
pub mod opensubtitles;
pub mod par2;
pub mod release;
pub mod search;
pub mod series;
pub mod settings;
pub mod space;
pub mod subtitles;
pub mod tmdb;
pub mod tvmaze;
pub mod yenc;

pub use error::{Error, Result};
pub use settings::{IndexerSettings, NewsServer, Settings, SubtitleSettings};
