//! Interface language: English (the default) or Turkish (`--lang tr`, `VEYLOG_LANG=tr`).
//!
//! The language is chosen once at the start of the program and stored in the thread (`set`);
//! everything that produces text reads it from here (`current`, `pick`). This way the `Display`
//! of the error types and the small functions that write numbers can write in both languages
//! without carrying a language parameter. Output is produced only on the main thread. Tests run
//! on their own threads: the language one test chooses does not affect the others.
//!
//! What does not change: the JSON output (field names, values such as `method` and placeholders
//! such as `<NUM>`) does not depend on the language and is in English, so that scripts do not
//! break when the language changes. Level names (`ERROR`) and times are the same in every
//! language.

use std::borrow::Cow;
use std::cell::Cell;

/// Interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// Turkish: decimal comma (`0,7`), percent sign first (`%1`).
    Tr,
    /// English, the default language: decimal point (`0.7`), percent sign last (`1%`).
    #[default]
    En,
}

impl Lang {
    /// All languages; the help text and error messages are generated from this list.
    pub const ALL: [Lang; 2] = [Lang::Tr, Lang::En];

    /// Its name on the command line: `tr`, `en`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tr => "tr",
            Lang::En => "en",
        }
    }

    /// The language with this name, ignoring case: `en`, `EN`, `tr`.
    pub fn parse(code: &str) -> Option<Lang> {
        Lang::ALL
            .into_iter()
            .find(|lang| lang.code().eq_ignore_ascii_case(code.trim()))
    }
}

thread_local! {
    /// The language of this thread. The program chooses it at the start (`set`); until then it is
    /// Turkish, because most unit tests check Turkish output. Tests that check English choose it
    /// with `scoped`.
    static LANG: Cell<Lang> = const { Cell::new(Lang::Tr) };
}

/// Chooses the language of the text produced on this thread.
pub fn set(lang: Lang) {
    LANG.with(|selected| selected.set(lang));
}

/// The selected language.
pub fn current() -> Lang {
    LANG.with(Cell::get)
}

/// In tests: chooses the language; when the returned guard is dropped (even if the test fails),
/// the previous language comes back.
#[cfg(test)]
#[must_use = "the language reverts when the guard is dropped"]
pub fn scoped(lang: Lang) -> LangGuard {
    let previous = current();
    set(lang);
    LangGuard(previous)
}

/// The guard `scoped` returns.
#[cfg(test)]
pub struct LangGuard(Lang);

#[cfg(test)]
impl Drop for LangGuard {
    fn drop(&mut self) {
        set(self.0);
    }
}

/// The text in the selected language: the Turkish one or the English one.
pub fn pick<T>(tr: T, en: T) -> T {
    match current() {
        Lang::Tr => tr,
        Lang::En => en,
    }
}

/// The name of an option in the selected language, so that messages show the name in the user's
/// language (`--son` → `--last`). An unknown name stays as it is.
pub fn option_name(name: &'static str) -> &'static str {
    if current() == Lang::Tr {
        return name;
    }
    OPTION_NAMES
        .iter()
        .find(|&&(tr, _)| tr == name)
        .map_or(name, |&(_, en)| en)
}

/// The Turkish and English names of the long options. Both work in every language; the help text
/// and the messages show the one in the selected language.
pub const OPTION_NAMES: [(&str, &str); 20] = [
    ("--esik", "--threshold"),
    ("--konum", "--location"),
    ("--ilk", "--top"),
    ("--hepsi", "--all"),
    ("--siralama", "--sort"),
    ("--bicim", "--format"),
    ("--karsilastir", "--compare"),
    ("--ani-artis", "--spikes"),
    ("--seviye", "--level"),
    ("--baslangic", "--since"),
    ("--bitis", "--until"),
    ("--son", "--last"),
    ("--saat-dilimi", "--timezone"),
    ("--desen", "--pattern"),
    ("--zaman-bicimi", "--time-format"),
    ("--izle", "--watch"),
    ("--aralik", "--interval"),
    ("--dil", "--lang"),
    ("--yardim", "--help"),
    ("--surum", "--version"),
];

/// Writes a decimal number in the selected language: with a comma in Turkish (`0,7`), with a
/// point in English (`0.7`). `text` is the number written with a point.
pub fn localize_decimal(text: String) -> String {
    match current() {
        Lang::Tr => text.replace('.', ","),
        Lang::En => text,
    }
}

/// A percentage: the sign comes first in Turkish (`%1`), last in English (`1%`).
pub fn percent(number: &str) -> String {
    match current() {
        Lang::Tr => format!("%{number}"),
        Lang::En => format!("{number}%"),
    }
}

/// A number and a noun: in Turkish the noun after a number does not change (`3 kayıt`); in
/// English it is singular or plural depending on the number (`1 record`, `3 records`).
pub fn counted(n: usize, tr: &str, singular: &str, plural: &str) -> String {
    match current() {
        Lang::Tr => format!("{n} {tr}"),
        Lang::En if n == 1 => format!("{n} {singular}"),
        Lang::En => format!("{n} {plural}"),
    }
}

/// "a", "a ve b", "a, b ve c"; in English "a and b", "a, b and c".
pub fn list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [single] => single.as_ref().to_owned(),
        [init @ .., last] => {
            let init: Vec<&str> = init.iter().map(AsRef::as_ref).collect();
            format!(
                "{} {} {}",
                init.join(", "),
                pick("ve", "and"),
                last.as_ref()
            )
        }
    }
}

/// The placeholders of masking: their Turkish names (in storage and in the Turkish interface) and
/// their English names (in the English interface and in JSON). `<URL>`, `<UUID>` and `<IP>` are
/// the same in both languages.
const PLACEHOLDERS: [(&str, &str); 4] = [
    ("<SAYI>", "<NUM>"),
    ("<YOL>", "<PATH>"),
    ("<EPOSTA>", "<EMAIL>"),
    ("<ZAMAN>", "<TIME>"),
];

/// Shows the placeholders of a pattern in the selected language: in English `<SAYI>` → `<NUM>`.
/// Patterns are stored and clustered with the Turkish names; the translation happens only in the
/// output.
pub fn placeholders(pattern: &str) -> Cow<'_, str> {
    if current() == Lang::Tr {
        return Cow::Borrowed(pattern);
    }
    english_placeholders(pattern)
}

/// The English names of the placeholders in a pattern, whatever the selected language: JSON
/// writes these in every language.
pub fn english_placeholders(pattern: &str) -> Cow<'_, str> {
    if !pattern.contains('<') {
        return Cow::Borrowed(pattern);
    }
    let mut result = Cow::Borrowed(pattern);
    for (tr, en) in PLACEHOLDERS {
        if result.contains(tr) {
            result = Cow::Owned(result.replace(tr, en));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse(" EN "), Some(Lang::En));
        assert_eq!(Lang::parse("Tr"), Some(Lang::Tr));
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::parse(""), None);
        for lang in Lang::ALL {
            assert_eq!(Lang::parse(lang.code()), Some(lang));
        }
    }

    #[test]
    fn language_is_per_thread() {
        assert_eq!(current(), Lang::Tr);
        {
            let _lang = scoped(Lang::En);
            assert_eq!(pick("Kayıt", "Records"), "Records");
            // Another thread starts with the default language
            let other = std::thread::spawn(|| (current(), pick(1, 2)))
                .join()
                .unwrap();
            assert_eq!(other, (Lang::Tr, 1));
        }
        // The guard was dropped: the previous language is back
        assert_eq!(pick("Kayıt", "Records"), "Kayıt");
    }

    #[test]
    fn numbers_and_lists_follow_the_language() {
        assert_eq!(localize_decimal("0.7".to_owned()), "0,7");
        assert_eq!(percent("1"), "%1");
        assert_eq!(list(&["a", "b", "c"]), "a, b ve c");
        assert_eq!(counted(1, "kayıt", "record", "records"), "1 kayıt");
        assert_eq!(counted(2, "kayıt", "record", "records"), "2 kayıt");
        assert_eq!(list(&["a"]), "a");
        assert_eq!(list::<&str>(&[]), "");
        let _lang = scoped(Lang::En);
        assert_eq!(localize_decimal("0.7".to_owned()), "0.7");
        assert_eq!(percent("1"), "1%");
        assert_eq!(list(&["a", "b"]), "a and b");
        assert_eq!(list(&["a", "b", "c"]), "a, b and c");
        assert_eq!(counted(1, "kayıt", "record", "records"), "1 record");
        assert_eq!(counted(0, "kayıt", "record", "records"), "0 records");
        assert_eq!(counted(2, "kayıt", "record", "records"), "2 records");
    }

    #[test]
    fn option_names() {
        assert_eq!(option_name("--son"), "--son");
        {
            let _lang = scoped(Lang::En);
            assert_eq!(option_name("--son"), "--last");
            assert_eq!(option_name("--izle"), "--watch");
            // An unknown name stays as it is
            assert_eq!(option_name("--x"), "--x");
        }
        // Every Turkish name and every English name appears once
        let mut names: Vec<&str> = OPTION_NAMES.iter().flat_map(|&(tr, en)| [tr, en]).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn placeholders_are_translated_in_english() {
        let pattern = "User <SAYI> logged in: <YOL> <EPOSTA> <ZAMAN> <IP> <URL> <*>";
        assert!(matches!(placeholders(pattern), Cow::Borrowed(_)));
        let _lang = scoped(Lang::En);
        assert_eq!(
            placeholders(pattern),
            "User <NUM> logged in: <PATH> <EMAIL> <TIME> <IP> <URL> <*>"
        );
        assert!(matches!(placeholders("<IP>:<*>"), Cow::Borrowed(_)));
        assert!(matches!(placeholders("plain text"), Cow::Borrowed(_)));
    }
}
