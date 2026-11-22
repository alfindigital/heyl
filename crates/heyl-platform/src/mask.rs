//! `heyl run`'s output filter: protected values become stars.
//!
//! A child given secrets in its environment can print them — an `env` dump, a
//! verbose connection string, a failing `curl -v` — and its output ends up in
//! CI logs and agent transcripts. [`Masker`] is a streaming search-and-replace
//! over the bytes the child writes: every occurrence of a secret becomes as
//! many `*` as the secret has characters.
//!
//! It works on raw bytes, so output that is not UTF-8 passes through intact.
//! A secret may arrive split across two reads; the masker holds back a tail
//! that could still turn into one until the next read decides it. It is a
//! safety net, not a guarantee: a value printed encoded (base64, URL escapes)
//! or in pieces is not recognised. DESIGN.md §5 (`run`) holds the rules.

use zeroize::Zeroizing;

/// Values with fewer characters than this are not masked: starring every `1`
/// or `prod` in a build log would hide far more than it protects.
pub const MIN_CHARS: usize = 4;

/// One value to mask.
struct Secret {
    bytes: Zeroizing<Vec<u8>>,
    /// How many stars it becomes: its character count, not its byte count.
    stars: usize,
}

/// A streaming filter replacing secrets by stars.
pub struct Masker {
    /// Longest first, so the longest secret matching at a position wins.
    secrets: Vec<Secret>,
    /// Output not yet emitted: a tail that may still be the start of a secret.
    pending: Zeroizing<Vec<u8>>,
}

impl Masker {
    /// A masker for `secrets`. Values shorter than [`MIN_CHARS`] characters
    /// are ignored, as are duplicates.
    pub fn new<'a>(secrets: impl IntoIterator<Item = &'a str>) -> Self {
        let mut list: Vec<Secret> = Vec::new();
        for secret in secrets {
            let stars = secret.chars().count();
            if stars < MIN_CHARS || list.iter().any(|s| s.bytes.as_slice() == secret.as_bytes()) {
                continue;
            }
            list.push(Secret {
                bytes: Zeroizing::new(secret.as_bytes().to_vec()),
                stars,
            });
        }
        list.sort_by(|a, b| b.bytes.len().cmp(&a.bytes.len()));
        Self {
            secrets: list,
            pending: Zeroizing::new(Vec::new()),
        }
    }

    /// Whether there is nothing to mask — the output can bypass the filter.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Feed a chunk of output; returns what can be emitted now.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        self.drain(false)
    }

    /// End of stream: returns everything still held. A held tail that never
    /// completed a secret is not one, and passes through as it is.
    pub fn finish(&mut self) -> Vec<u8> {
        self.drain(true)
    }

    fn drain(&mut self, last: bool) -> Vec<u8> {
        let data = self.pending.as_slice();
        let mut out = Vec::with_capacity(data.len());
        let mut i = 0;
        'scan: while i < data.len() {
            let rest = &data[i..];
            // Longest first: a longer secret that may still complete here
            // must be waited for before a shorter one that already matches
            // is starred — otherwise the longer one's remainder leaks.
            for secret in &self.secrets {
                let s = secret.bytes.as_slice();
                if rest.len() >= s.len() {
                    if rest.starts_with(s) {
                        out.resize(out.len() + secret.stars, b'*');
                        i += s.len();
                        continue 'scan;
                    }
                } else if !last && s.starts_with(rest) {
                    break 'scan;
                }
            }
            out.push(data[i]);
            i += 1;
        }
        self.pending.drain(..i);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(secrets: &[&str], chunks: &[&[u8]]) -> Vec<u8> {
        let mut m = Masker::new(secrets.iter().copied());
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(m.push(chunk));
        }
        out.extend(m.finish());
        out
    }

    fn masked(secrets: &[&str], text: &str) -> String {
        String::from_utf8(mask(secrets, &[text.as_bytes()])).expect("utf-8")
    }

    #[test]
    fn a_secret_becomes_as_many_stars() {
        assert_eq!(masked(&["hunter22"], "pw=hunter22\n"), "pw=********\n");
    }

    #[test]
    fn every_occurrence_of_every_secret_is_masked() {
        assert_eq!(
            masked(&["alpha1", "beta22"], "alpha1 beta22 alpha1beta22"),
            "****** ****** ************"
        );
    }

    #[test]
    fn the_longest_secret_wins() {
        assert_eq!(masked(&["abcd", "abcdefgh"], "xabcdefghx"), "x********x");
        assert_eq!(masked(&["abcd", "abcdefgh"], "xabcdefx"), "x****efx");
        // Overlapping, not nested: the earlier start wins.
        assert_eq!(masked(&["abcdef", "defghi"], "abcdefghi"), "******ghi");
    }

    #[test]
    fn a_secret_split_anywhere_is_still_masked() {
        let text = b"before hunter22 after";
        for cut in 0..=text.len() {
            let (a, b) = text.split_at(cut);
            assert_eq!(
                mask(&["hunter22"], &[a, b]),
                b"before ******** after",
                "cut {cut}"
            );
        }
        // Byte by byte.
        let chunks: Vec<&[u8]> = text.chunks(1).collect();
        assert_eq!(mask(&["hunter22"], &chunks), b"before ******** after");
    }

    #[test]
    fn a_longer_secret_split_after_a_shorter_one_is_waited_for() {
        assert_eq!(
            mask(&["abcd", "abcdefgh"], &[b"abcd", b"efgh"]),
            b"********"
        );
    }

    #[test]
    fn only_a_possible_start_is_held_back() {
        let mut m = Masker::new(["hunter22"]);
        assert_eq!(m.push(b"say hun"), b"say ");
        assert_eq!(m.push(b"gry"), b"hungry");
        assert_eq!(m.push(b" hunt"), b" ");
        // The stream ends mid-way: an incomplete secret is not a secret.
        assert_eq!(m.finish(), b"hunt");
    }

    #[test]
    fn stars_count_characters_not_bytes() {
        assert_eq!(masked(&["pässwörd"], "[pässwörd]"), "[********]");
    }

    #[test]
    fn output_that_is_not_utf8_passes_through() {
        let out = mask(&["hunter22"], &[b"\xff\xfehunter22\x80"]);
        assert_eq!(out, b"\xff\xfe********\x80");
    }

    #[test]
    fn short_empty_and_duplicate_values_are_dropped() {
        let m = Masker::new(["", "1", "abc", "äöü"]);
        assert!(m.is_empty());
        assert_eq!(masked(&["abc", "abcd"], "abc abcd"), "abc ****");
        assert_eq!(masked(&["äöüß"], "äöüß"), "****");
        assert_eq!(Masker::new(["same", "same"]).secrets.len(), 1);
    }

    #[test]
    fn nothing_to_mask_changes_nothing() {
        assert_eq!(masked(&[], "anything at all"), "anything at all");
    }
}
