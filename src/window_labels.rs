// Short labels that tell apart windows of the same application.
//
// created: 15.11.2026, PE
//
// SPDX-License-Identifier: GPL-3.0-only

//! The label of a window is the part of its title that differs from the titles
//! of the other windows of the same application. Text shared by all those
//! titles is dropped, and the remaining parts are joined with an ellipsis, so
//! "notes.md - nixos - VS Code" next to "docs.md - repos - VS Code" becomes
//! "notes…nix…" and "docs…repos".

/// Longest label in characters, joining and trailing ellipses included.
pub const MAX_LABEL_CHARS: usize = 10;
/// Shorter common runs are coincidence ("o" in "notes" and "docs"), not shared text.
const MIN_SHARED_CHARS: usize = 3;
/// A later part cut shorter than this carries no information and is dropped.
const MIN_CUT_PART_CHARS: usize = 3;
const ELLIPSIS: &str = "…";

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '|' | ':' | '—' | '–' | '·' | '•' | ',')
}

/// Longest common run of `a[alo..ahi]` and `b[blo..bhi]` as `(a_start, b_start, len)`.
///
/// Ties go to the earliest run in `a`, then in `b`, so results are deterministic.
fn longest_match(
    a: &[char],
    b: &[char],
    (alo, ahi): (usize, usize),
    (blo, bhi): (usize, usize),
) -> (usize, usize, usize) {
    let mut best = (alo, blo, 0);
    // Entry j - blo + 1 holds the length of the common run ending at a[i] and
    // b[j], kept one row of `a` at a time.
    let mut previous = vec![0usize; bhi - blo + 1];
    for (i, a_char) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut current = vec![0usize; bhi - blo + 1];
        for j in blo..bhi {
            if *a_char == b[j] {
                let len = previous[j - blo] + 1;
                current[j - blo + 1] = len;
                if len > best.2 {
                    best = (i + 1 - len, j + 1 - len, len);
                }
            }
        }
        previous = current;
    }
    best
}

/// Common runs of `a` and `b` in order, found by splitting around the longest run.
fn matching_blocks(a: &[char], b: &[char]) -> Vec<(usize, usize, usize)> {
    let mut blocks = Vec::new();
    let mut pending = vec![((0, a.len()), (0, b.len()))];
    while let Some((a_range, b_range)) = pending.pop() {
        let (i, j, len) = longest_match(a, b, a_range, b_range);
        if len == 0 {
            continue;
        }
        blocks.push((i, j, len));
        pending.push(((a_range.0, i), (b_range.0, j)));
        pending.push(((i + len, a_range.1), (j + len, b_range.1)));
    }
    blocks.sort_unstable();
    blocks
}

/// Runs of text that occur, in this order, in every title.
fn shared_runs(titles: &[Vec<char>]) -> Vec<Vec<char>> {
    let Some((first, rest)) = titles.split_first() else {
        return Vec::new();
    };
    let mut shared = vec![first.clone()];
    for title in rest {
        // A separator that no title contains keeps runs from merging across gaps.
        let joined = shared.join(&'\0');
        shared = matching_blocks(&joined, title)
            .into_iter()
            .filter(|&(_, _, len)| len >= MIN_SHARED_CHARS)
            .map(|(i, _, len)| joined[i..i + len].to_vec())
            .collect();
    }
    shared
}

fn find_run(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    (from..=haystack.len().checked_sub(needle.len())?)
        .find(|&start| haystack[start..start + needle.len()] == *needle)
}

/// Parts of `title` outside the shared runs, widened to whole words and trimmed.
fn distinct_parts(title: &[char], shared: &[Vec<char>]) -> Vec<String> {
    let mut distinct = vec![true; title.len()];
    let mut cursor = 0;
    for run in shared {
        if let Some(start) = find_run(title, run, cursor) {
            distinct[start..start + run.len()].fill(false);
            cursor = start + run.len();
        }
    }

    // A word with any distinct letter counts as distinct as a whole, so labels
    // read "notes" rather than "note" when "s.md" happens to be shared.
    let mut word_start = 0;
    for i in 0..=title.len() {
        if i == title.len() || !title[i].is_alphanumeric() {
            if distinct[word_start..i].contains(&true) {
                distinct[word_start..i].fill(true);
            }
            word_start = i + 1;
        }
    }

    let mut parts = Vec::new();
    let mut part = String::new();
    for (&c, &is_distinct) in title.iter().zip(&distinct) {
        if is_distinct {
            part.push(c);
        } else if !part.is_empty() {
            parts.push(std::mem::take(&mut part));
        }
    }
    parts.push(part);
    parts
        .iter()
        .map(|part| part.trim_matches(is_separator))
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Joins parts with ellipses within `MAX_LABEL_CHARS`, ending in "…" when cut.
fn fit_label(parts: &[String]) -> String {
    let full = parts.join(ELLIPSIS);
    if full.chars().count() <= MAX_LABEL_CHARS {
        return full;
    }

    // Keep one character for the trailing ellipsis.
    let budget = MAX_LABEL_CHARS - 1;
    let mut label = String::new();
    let mut used = 0;
    for (index, part) in parts.iter().enumerate() {
        let joiner = usize::from(index > 0);
        let room = budget.saturating_sub(used + joiner);
        let kept: String = part.chars().take(room).collect();
        let kept = kept.trim_end_matches(is_separator);
        let kept_len = kept.chars().count();
        let whole = part.chars().count() <= room;
        if whole || index == 0 || kept_len >= MIN_CUT_PART_CHARS {
            if index > 0 {
                label.push_str(ELLIPSIS);
            }
            label.push_str(kept);
            used += joiner + kept_len;
        }
        if !whole {
            break;
        }
    }
    label.push_str(ELLIPSIS);
    label
}

/// Labels for the windows of one application, in the order of `titles`.
///
/// A window gets no label when it is the app's only window, when its title is
/// empty or when nothing in its title differs from the others.
pub fn window_labels(titles: &[&str]) -> Vec<Option<String>> {
    if titles.len() < 2 {
        return vec![None; titles.len()];
    }
    let titles: Vec<Vec<char>> = titles
        .iter()
        .map(|title| title.trim().chars().filter(|&c| c != '\0').collect())
        .collect();
    let shared = shared_runs(&titles);
    titles
        .iter()
        .map(|title| {
            let parts = distinct_parts(title, &shared);
            (!parts.is_empty()).then(|| fit_label(&parts))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{MAX_LABEL_CHARS, fit_label, window_labels};

    fn labels(titles: &[&str]) -> Vec<Option<String>> {
        window_labels(titles)
    }

    fn some(label: &str) -> Option<String> {
        Some(label.to_owned())
    }

    #[test]
    fn joins_changed_parts_and_cuts_at_ten_characters_ellipses_included() {
        assert_eq!(
            labels(&["notes.md - nixos - VS Code", "docs.md - repos - VS Code"]),
            [some("notes…nix…"), some("docs…repos")]
        );
    }

    #[test]
    fn keeps_whole_words_when_shared_text_starts_inside_a_word() {
        assert_eq!(
            labels(&[
                "notes.md - nixos - Visual Studio Code",
                "docs.md - nixos - Visual Studio Code",
            ]),
            [some("notes"), some("docs")]
        );
        assert_eq!(
            labels(&["report_v1.pdf", "report_v2.pdf"]),
            [some("v1"), some("v2")]
        );
    }

    #[test]
    fn strips_separators_around_the_changed_parts() {
        assert_eq!(
            labels(&[
                "Slack | general | BreezeLabs",
                "Slack | data-team | BreezeLabs"
            ]),
            [some("general"), some("data-team")]
        );
        assert_eq!(
            labels(&[
                "GitHub - pop-os/cosmic-panel — Mozilla Firefox",
                "YouTube — Mozilla Firefox"
            ]),
            [some("GitHub…"), some("YouTube")]
        );
    }

    #[test]
    fn gives_identical_titles_no_label() {
        assert_eq!(
            labels(&["New Tab — Mozilla Firefox", "New Tab — Mozilla Firefox"]),
            [None, None]
        );
    }

    #[test]
    fn gives_a_title_that_is_only_the_shared_part_no_label() {
        assert_eq!(labels(&["Terminal", "Terminal - vim"]), [None, some("vim")]);
    }

    #[test]
    fn gives_single_windows_and_empty_titles_no_label() {
        assert_eq!(labels(&[]), Vec::<Option<String>>::new());
        assert_eq!(labels(&["Only window"]), [None]);
        assert_eq!(labels(&["", "Inbox"]), [None, some("Inbox")]);
        assert_eq!(labels(&["  ", "  "]), [None, None]);
    }

    #[test]
    fn drops_a_later_part_cut_below_three_characters() {
        assert_eq!(
            labels(&[
                "app.rs - workspace-icons - VS Code",
                "config.rs - workspace-icons - VS Code",
                "home.nix - nixos - VS Code",
            ]),
            [some("app.rs…"), some("config.rs…"), some("home.nix…")]
        );
    }

    #[test]
    fn cuts_a_long_first_part_and_ignores_trailing_separators() {
        assert_eq!(
            labels(&[
                "paul@thinkpad: ~/nixos",
                "paul@thinkpad: ~/repos/breeze_algos"
            ]),
            [some("nixos"), some("repos/bre…")]
        );
        assert_eq!(fit_label(&["abcdefgh - xyz".to_owned()]), "abcdefgh…");
    }

    #[test]
    fn never_exceeds_the_character_limit() {
        let titles = [
            "Ünïcödé wörds everywhere — Ärger",
            "Ganz andere Wörter hier — Ärger",
            "😀😀😀😀😀😀😀😀😀😀😀😀 — Ärger",
        ];
        for label in labels(&titles).into_iter().flatten() {
            assert!(label.chars().count() <= MAX_LABEL_CHARS, "{label}");
        }
    }

    #[test]
    fn ignores_nul_characters_in_titles() {
        assert_eq!(
            labels(&["a\0bcd one", "abcd two"]),
            [some("one"), some("two")]
        );
    }
}
