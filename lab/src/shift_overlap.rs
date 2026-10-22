//! Label-preserving shift carryover evaluation; never a candidate engine.
use std::collections::BTreeMap;

use tacit_core::lang::hangul::HangulComposer;
use tacit_core::session::{testing::FakeTextField, TextClient};
use tacit_core::{AppPolicies, DecisionContext, DecisionEngine, InputSession, Key, Languages};

use super::{prev_name, Case};

fn group(keys: &str) -> String {
    keys.to_ascii_lowercase()
}

fn split(group: &str) -> &'static str {
    if group == "akfgkfrk" || group == "dlfwlr" || super::tune::group_fold(group) != 0 {
        "dev"
    } else {
        "holdout"
    }
}

// Only the immediately following vowel carries Shift. O/P are meaningful vowels,
// so they are never flattened or synthesized as equivalent to o/p.
fn carryover(keys: &str, expected: &str) -> Vec<String> {
    let mut variants = Vec::new();
    for (i, pair) in keys.as_bytes().windows(2).enumerate() {
        if b"QWERTOP".contains(&pair[0]) && b"kijuhynbml".contains(&pair[1]) {
            let mut bytes = keys.as_bytes().to_vec();
            bytes[i + 1] = bytes[i + 1].to_ascii_uppercase();
            let variant = String::from_utf8(bytes).unwrap();
            if HangulComposer::compose(&variant).is_some_and(|c| c.text() == expected) {
                variants.push(variant);
            }
        }
    }
    variants
}

fn english_forms(word: &str) -> Vec<(&'static str, String)> {
    let mut title = word.to_string();
    title[..1].make_ascii_uppercase();
    vec![
        ("en.title", title.clone()),
        ("en.upper", word.to_ascii_uppercase()),
        ("en.camel-acronym", format!("{word}APIClient")),
        ("en.pascal-acronym", format!("{title}HTTPServer")),
        // These valid identifiers deliberately intersect the candidate's Shift
        // pattern, unlike API/HTTP controls rejected by an earlier capital.
        ("en.overlap-identifier", format!("{word}RI")),
        ("en.overlap-identifier", format!("{word}WL")),
        ("en.overlap-identifier", format!("RKey{title}")),
    ]
}

fn cases(japanese: bool) -> Vec<(String, Case)> {
    let mut rows = Vec::new();
    for case in super::word_cases() {
        let canonical = group(&case.keys);
        // Assign the group before making any perturbations or context variants.
        if case.expected.chars().all(|c| ('가'..='힣').contains(&c)) {
            for keys in carryover(&case.keys, &case.expected) {
                rows.push((
                    canonical.clone(),
                    Case {
                        bucket: format!("ko.carryover/{}", case.bucket),
                        keys,
                        previous: case.previous,
                        expected: case.expected.clone(),
                    },
                ));
            }
        }
        rows.push((canonical, case));
    }
    let terms = include_str!("../cases/terms.txt").lines().filter(|s| {
        !s.starts_with('#') && !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic())
    });
    for word in super::english_list().into_iter().chain(terms) {
        let canonical = group(word);
        for previous in [
            None,
            Some(super::Language::English),
            Some(super::Language::Korean),
        ] {
            let mut forms = english_forms(word);
            forms.push(("en.raw", word.to_string()));
            for (bucket, keys) in forms {
                rows.push((
                    canonical.clone(),
                    Case {
                        bucket: bucket.into(),
                        expected: keys.clone(),
                        keys,
                        previous,
                    },
                ));
            }
        }
    }
    for (original, variant, expected) in [
        ("akfgkfRk", "akfgkfRK", "말할까"),
        ("dlfWlr", "dlfWLr", "일찍"),
    ] {
        for keys in [original, variant] {
            assert_eq!(HangulComposer::compose(keys).unwrap().text(), expected);
            for previous in [
                None,
                Some(super::Language::English),
                Some(super::Language::Korean),
            ] {
                rows.push((
                    group(original),
                    Case {
                        bucket: "ko.issue12".into(),
                        keys: keys.into(),
                        previous,
                        expected: expected.into(),
                    },
                ));
            }
        }
    }
    if japanese {
        // These fixtures label kana readings. Kanji ranking remains in --japanese;
        // this replay uses the session's default KanaConverter, not learned conversion.
        for fields in super::data_lines(include_str!("../cases/japanese.tsv")) {
            assert_eq!(fields.len(), 3);
            // Mixed Latin/kana conversion fixtures have no single kana spelling;
            // they remain covered by the existing Japanese conversion evaluator.
            if fields[0] == "mixed" {
                continue;
            }
            let keys = tacit_core::RomajiComposer::romaji(fields[1])
                .expect("Japanese fixture must have a romaji spelling");
            // Japanese Space opens conversion rather than committing a space;
            // keep these as contextual decision-only controls, not delimiter rows.
            rows.push((
                group(&keys),
                Case {
                    bucket: format!("ja.reading/{}", fields[0]),
                    keys,
                    previous: Some(super::Language::Japanese),
                    expected: fields[1].into(),
                },
            ));
        }
    }
    rows
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn identity(profile: &str, group: &str, case: &Case) -> String {
    // Length framing is unambiguous even when a source contains separators.
    [
        profile,
        group,
        prev_name(case.previous),
        &case.bucket,
        &case.keys,
    ]
    .iter()
    .map(|s| format!("{}:{s}", s.len()))
    .collect()
}

fn replay_session(
    engine: &std::sync::Arc<tacit_core::LearningEngine>,
    keys: &str,
    delimiter: Key,
) -> (String, String) {
    let mut session = InputSession::new(engine.clone(), AppPolicies::default(), true);
    let mut client = FakeTextField::new();
    for ch in keys.chars() {
        let key = if ch.is_ascii_alphabetic() {
            Key::Letter(ch)
        } else {
            Key::Printable(ch)
        };
        if !session.handle(key, &mut client) {
            client.insert_text(&ch.to_string(), None);
        }
    }
    let range = client.marked_range();
    let mark = if range.location == tacit_core::session::NOT_FOUND {
        String::new()
    } else {
        client
            .text_in(range)
            .expect("marked range must be readable")
    };
    let host_text = match delimiter {
        Key::Enter => "\n".to_string(),
        Key::Printable(ch) => ch.to_string(),
        _ => panic!("session replay needs a printable delimiter or Enter"),
    };
    if !session.handle(delimiter, &mut client) {
        client.insert_text(&host_text, None);
    }
    (mark, client.text())
}

pub fn run(args: &[String], profile: &str, languages: &Languages) {
    assert!(
        !std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("LAB_ENKO")),
        "--shift-overlap refuses LAB_ENKO experimental overrides"
    );
    let selected = args
        .iter()
        .position(|a| a == "--shift-split")
        .map(|i| {
            args.get(i + 1)
                .expect("--shift-split needs dev|holdout|all")
                .as_str()
        })
        .unwrap_or("dev");
    assert!(
        matches!(selected, "dev" | "holdout" | "all"),
        "invalid --shift-split"
    );
    let engine = super::make_engine(None, languages);
    let mut rows = BTreeMap::new();
    for (group, case) in cases(languages.contains(super::Language::Japanese)) {
        if selected != "all" && selected != split(&group) {
            continue;
        }
        let id = identity(profile, &group, &case);
        match rows.entry(id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((group, case));
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                // Source-list duplicates are collapsed, never conflicting labels.
                let (_, old): &(String, Case) = entry.get();
                assert_eq!(old.expected, case.expected, "conflicting source labels");
            }
        }
    }
    println!("id\tprofile\tsplit\tgroup\tbucket\tkeys\texpected\tprevious\tscope\tpartial\tfinal\tmark\tenter\tspace");
    for (id, (group, case)) in rows {
        let context = |partial| DecisionContext {
            previous_language: case.previous,
            partial,
            ..Default::default()
        };
        let partial = engine.decide(&case.keys, &context(true)).output;
        let final_output = engine.decide(&case.keys, &context(false)).output;
        let mut mark = String::new();
        let mut enter = String::new();
        let mut space = String::new();
        // Contextual decide rows deliberately do not claim contextual session replay.
        let scope = if case.previous.is_none() {
            "decision+session"
        } else {
            "decision"
        };
        if case.previous.is_none() {
            (mark, enter) = replay_session(&engine, &case.keys, Key::Enter);
            (_, space) = replay_session(&engine, &case.keys, Key::Printable(' '));
        }
        let fields = [
            id,
            profile.into(),
            split(&group).into(),
            group,
            case.bucket,
            case.keys,
            case.expected,
            prev_name(case.previous).into(),
            scope.into(),
            partial,
            final_output,
            mark,
            enter,
            space,
        ];
        println!(
            "{}",
            fields
                .iter()
                .map(|s| escape(s))
                .collect::<Vec<_>>()
                .join("\t")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_oracle_observes_mark_and_host_delimiters_once() {
        let engine = super::super::make_engine(None, &Languages::legacy(false));
        for (delimiter, expected) in [
            (Key::Enter, "hello"),
            (Key::Printable(' '), "hello "),
            (Key::Printable('!'), "hello!"),
        ] {
            let (mark, confirmed) = replay_session(&engine, "hello", delimiter);
            assert_eq!(mark, "hello");
            assert_eq!(confirmed, expected);
        }
        let (mark, confirmed) = replay_session(&engine, "!hello", Key::Enter);
        assert_eq!(mark, "hello");
        assert_eq!(confirmed, "!hello");
    }

    #[test]
    fn all_nonmixed_japanese_readings_build() {
        let fixtures: Vec<_> = super::super::data_lines(include_str!("../cases/japanese.tsv"))
            .filter(|fields| fields[0] != "mixed")
            .collect();
        assert_eq!(fixtures.len(), 60);
        let rows = cases(true);
        let readings: Vec<_> = rows
            .iter()
            .filter(|(_, case)| case.bucket.starts_with("ja.reading/"))
            .collect();
        assert_eq!(readings.len(), fixtures.len());
        for fields in fixtures {
            assert!(readings.iter().any(|(_, case)| {
                case.bucket == format!("ja.reading/{}", fields[0])
                    && case.expected == fields[1]
                    && case.previous == Some(super::super::Language::Japanese)
                    && tacit_core::RomajiComposer::romaji(fields[1]).as_deref()
                        == Some(case.keys.as_str())
            }));
        }
    }

    #[test]
    fn carryover_preserves_composition() {
        for word in ["일찍", "말할까", "예쁘다", "까치"] {
            let keys = super::super::keys_of(word);
            for variant in carryover(&keys, word) {
                assert_eq!(HangulComposer::compose(&variant).unwrap().text(), word);
            }
        }
        assert!(carryover("dlfWlr", "일찍").contains(&"dlfWLr".into()));
        assert!(carryover("akfgkfRk", "말할까").contains(&"akfgkfRK".into()));
    }

    #[test]
    fn meaningful_vowels_are_not_normalized() {
        assert_ne!(
            HangulComposer::compose("rO").unwrap().text(),
            HangulComposer::compose("ro").unwrap().text()
        );
        assert_ne!(
            HangulComposer::compose("rP").unwrap().text(),
            HangulComposer::compose("rp").unwrap().text()
        );
        assert!(carryover("Ro", "깨").is_empty());
        assert!(carryover("Rp", "께").is_empty());
    }

    #[test]
    fn canonical_partition_and_raw_english_contract() {
        for key in ["akfgkfRk", "akfgkfRK", "dlfWlr", "dlfWLr"] {
            assert_eq!(split(&group(key)), "dev");
        }
        let rows = cases(false);
        for (canonical, case) in &rows {
            if case.bucket.starts_with("en.") {
                assert_eq!(case.keys, case.expected);
            }
            if split(canonical) == "holdout" {
                assert_eq!(super::super::tune::group_fold(canonical), 0);
            }
        }
        assert!(rows
            .iter()
            .any(|(_, case)| case.keys == "dlfWLr" && case.expected == "일찍"));
        assert!(rows
            .iter()
            .any(|(_, case)| case.bucket == "en.upper" && case.keys == case.expected));
        assert!(english_forms("http").contains(&("en.upper", "HTTP".into())));
    }
}
