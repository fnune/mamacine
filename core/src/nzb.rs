//! What an nzb says about its own chances.

use crate::error::{Error, Result};

/// One posted file, with its own articles.
#[derive(Clone, Debug, PartialEq)]
pub struct PostedFile {
    /// From the subject's quotes; None when hidden.
    pub name: Option<String>,
    pub repair: bool,
    pub bytes: u64,
    pub ids: Vec<String>,
}

/// One nzb, before any of it is fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct Contents {
    pub files: Vec<PostedFile>,
    /// Data article ids, in posting order, unbracketed.
    pub data_ids: Vec<String>,
    /// Repair article ids; takedowns hit these first.
    pub par_ids: Vec<String>,
    pub data_bytes: u64,
    pub par_bytes: u64,
}

impl Contents {
    /// The share of the post that repairs.
    pub fn par_ratio(&self) -> f64 {
        if self.data_bytes == 0 {
            return 0.0;
        }
        self.par_bytes as f64 / self.data_bytes as f64
    }

    /// Evenly spaced, deterministic sample of data articles.
    pub fn sample(&self, wanted: usize) -> Vec<&str> {
        spaced(&self.data_ids, wanted)
    }

    /// The same, over the repair articles.
    pub fn sample_par(&self, wanted: usize) -> Vec<&str> {
        spaced(&self.par_ids, wanted)
    }

    /// The data sample, with each article's file.
    pub fn sample_with_files(&self, wanted: usize) -> Vec<(usize, &str)> {
        let mut owned: Vec<(usize, &str)> = Vec::new();
        for (position, file) in self.files.iter().enumerate() {
            if file.repair {
                continue;
            }
            owned.extend(file.ids.iter().map(|id| (position, id.as_str())));
        }
        if owned.is_empty() || wanted == 0 {
            return Vec::new();
        }
        let step = (owned.len() as f64 / wanted as f64).max(1.0);
        let mut picked = Vec::new();
        let mut at = 0.0;
        while (at as usize) < owned.len() && picked.len() < wanted {
            picked.push(owned[at as usize]);
            at += step;
        }
        picked
    }

    /// Articles that carry the repair set's contents.
    pub fn par_index_segments(&self) -> Vec<&str> {
        let mut repair: Vec<&PostedFile> = self.files.iter().filter(|file| file.repair).collect();
        repair.sort_by_key(|file| {
            let volume = file
                .name
                .as_deref()
                .map(|name| name.contains("vol"))
                .unwrap_or(true);
            (volume, file.bytes)
        });
        repair
            .iter()
            .take(2)
            .filter_map(|file| file.ids.first().map(String::as_str))
            .collect()
    }

    /// Paper coverage, discounted by missing repair data.
    pub fn effective_par(&self, par_missing_ratio: f64) -> f64 {
        self.par_ratio() * (1.0 - par_missing_ratio).max(0.0)
    }
}

fn spaced(ids: &[String], wanted: usize) -> Vec<&str> {
    if ids.is_empty() || wanted == 0 {
        return Vec::new();
    }
    let step = (ids.len() as f64 / wanted as f64).max(1.0);
    let mut picked = Vec::new();
    let mut at = 0.0;
    while (at as usize) < ids.len() && picked.len() < wanted {
        picked.push(ids[at as usize].as_str());
        at += step;
    }
    picked
}

pub fn read(nzb: &[u8]) -> Result<Contents> {
    let text = String::from_utf8_lossy(nzb);
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let document = roxmltree::Document::parse_with_options(&text, options).map_err(|failure| {
        Error::Unreadable {
            what: "the nzb file".into(),
            detail: failure.to_string(),
        }
    })?;

    let mut contents = Contents {
        files: Vec::new(),
        data_ids: Vec::new(),
        par_ids: Vec::new(),
        data_bytes: 0,
        par_bytes: 0,
    };
    for file in document
        .descendants()
        .filter(|node| node.has_tag_name("file"))
    {
        let subject = file.attribute("subject").unwrap_or_default().to_lowercase();
        let repair = subject.contains(".par2");
        let mut posted = PostedFile {
            name: quoted_name(&subject),
            repair,
            bytes: 0,
            ids: Vec::new(),
        };
        for segment in file
            .descendants()
            .filter(|node| node.has_tag_name("segment"))
        {
            let bytes: u64 = segment
                .attribute("bytes")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            posted.bytes += bytes;
            if let Some(id) = segment.text() {
                posted.ids.push(id.trim().to_string());
            }
            if repair {
                contents.par_bytes += bytes;
                if let Some(id) = segment.text() {
                    contents.par_ids.push(id.trim().to_string());
                }
            } else {
                contents.data_bytes += bytes;
                if let Some(id) = segment.text() {
                    contents.data_ids.push(id.trim().to_string());
                }
            }
        }
        contents.files.push(posted);
    }
    Ok(contents)
}

fn quoted_name(subject: &str) -> Option<String> {
    let start = subject.find('"')? + 1;
    let end = subject[start..].find('"')? + start;
    Some(subject[start..end].to_lowercase())
}

/// Conservative: anything uncertain goes to nzbget.
pub fn beyond_repair(missing_ratio: f64, par_ratio: f64) -> bool {
    missing_ratio > 0.01 && missing_ratio > par_ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nzb() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE nzb PUBLIC "-//newzbin//DTD NZB 1.1//EN" "http://www.newzbin.com/DTD/nzb/nzb-1.1.dtd">
        <nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
          <file subject="&quot;Show.S01.part01.rar&quot; yEnc (1/3)">
            <segments>
              <segment bytes="500000" number="1">data-1@news</segment>
              <segment bytes="500000" number="2">data-2@news</segment>
              <segment bytes="500000" number="3">data-3@news</segment>
            </segments>
          </file>
          <file subject="&quot;Show.S01.vol00+01.PAR2&quot; yEnc (1/1)">
            <segments>
              <segment bytes="80000" number="1">par-vol@news</segment>
            </segments>
          </file>
          <file subject="&quot;Show.S01.par2&quot; yEnc (1/1)">
            <segments>
              <segment bytes="20000" number="1">par-index@news</segment>
            </segments>
          </file>
        </nzb>"#
            .to_string()
    }

    #[test]
    fn reads_the_articles_and_tells_repair_data_from_the_film() {
        let contents = read(nzb().as_bytes()).expect("readable");
        assert_eq!(
            contents.data_ids,
            ["data-1@news", "data-2@news", "data-3@news"]
        );
        assert_eq!(contents.par_ids, ["par-vol@news", "par-index@news"]);
        assert_eq!(
            contents.files[0].name.as_deref(),
            Some("show.s01.part01.rar"),
            "the name the repair set knows the file by"
        );
        assert_eq!(
            contents.par_index_segments(),
            ["par-index@news", "par-vol@news"],
            "the index first, the smallest volume as the fallback source of the same packets"
        );
        let owned = contents.sample_with_files(2);
        assert!(
            owned.iter().all(|(file, _)| *file == 0),
            "damage names its file"
        );
        assert_eq!(contents.data_bytes, 1_500_000);
        assert_eq!(contents.par_bytes, 100_000);
        assert!((contents.par_ratio() - 0.0667).abs() < 0.001);
    }

    #[test]
    fn repair_data_that_is_gone_repairs_nothing() {
        let contents = Contents {
            files: vec![],
            data_ids: vec![],
            par_ids: vec![],
            data_bytes: 100,
            par_bytes: 10,
        };
        assert!((contents.effective_par(0.9) - 0.01).abs() < 1e-9);
        assert!(
            beyond_repair(0.017, contents.effective_par(0.9)),
            "the Joy copy"
        );
        assert!(
            !beyond_repair(0.017, contents.effective_par(0.0)),
            "with its par intact it would have been fine"
        );
    }

    #[test]
    fn the_sample_is_evenly_spaced_and_never_larger_than_asked() {
        let contents = Contents {
            files: vec![],
            data_ids: (0..100).map(|n| format!("id-{n}")).collect(),
            par_ids: vec![],
            data_bytes: 100,
            par_bytes: 0,
        };
        let sample = contents.sample(10);
        assert_eq!(sample.len(), 10);
        assert_eq!(sample[0], "id-0");
        assert_eq!(
            sample[9], "id-90",
            "spread across the whole post, not the front"
        );
        assert!(
            contents.sample(1000).len() == 100,
            "asking for more than exists is fine"
        );
        assert!(Contents {
            files: vec![],
            data_ids: vec![],
            par_ids: vec![],
            data_bytes: 0,
            par_bytes: 0
        }
        .sample(10)
        .is_empty());
    }

    #[test]
    fn a_copy_whose_loss_exceeds_its_repair_headroom_is_beyond_saving() {
        assert!(beyond_repair(0.061, 0.060));
        assert!(
            !beyond_repair(0.02, 0.06),
            "repairable damage is nzbget's ordinary day"
        );
        assert!(
            !beyond_repair(0.009, 0.0),
            "a sliver of sampling noise never skips a copy"
        );
        assert!(beyond_repair(0.5, 0.1));
    }

    #[test]
    fn an_unreadable_nzb_is_an_error_rather_than_an_empty_answer() {
        assert!(read(b"not xml at all").is_err());
    }
}
