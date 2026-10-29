//! Replaces the variable parts of messages (numbers, IP addresses, email addresses...) with
//! placeholders.
//!
//! The goal is to turn messages of the same kind into the same text:
//!
//! ```text
//! User 381 logged in   →  User <NUM> logged in
//! User 1024 logged in  →  User <NUM> logged in
//! ```
//!
//! This is the first stage of the Drain algorithm: when the variables are cleaned up first, the
//! patterns are found much more accurately.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// A masking rule: every part that matches the pattern is replaced with the placeholder.
struct Rule {
    pattern: Regex,
    /// Can refer to the pattern's groups as `${1}`: the text the group captured stays in place.
    placeholder: &'static str,
    /// If the character right after the match meets this condition, the match is not masked. The
    /// regex crate has no lookahead; if the next character were part of the pattern, it would be
    /// consumed and could not be the separator at the start of a second path right after the
    /// first.
    not_followed_by: Option<fn(char) -> bool>,
}

/// The rules are applied in order, from top to bottom, and the order matters: all the structures
/// (URL, email, IP...) must be caught first, and single numbers must come last. Otherwise
/// "10.0.3.12" would be masked as numbers split apart, not as an IP address.
///
/// `LazyLock`: the regular expressions are compiled once, on first use, and the same ones are used
/// from then on.
///
/// Word boundaries (`(?-u:\b)`) and digits (`[0-9]`) are ASCII: a Unicode word boundary makes the
/// regex crate switch from its fast engine to a much slower one at every non-ASCII byte; masking
/// messages with non-ASCII text (Turkish messages, for example) would be four times slower. The
/// difference is only in digits stuck to a non-ASCII letter: the digit in `Café5` and `4ème` is
/// masked (the one in `oauth2` is not).
///
/// A URL ends at whitespace, at a closing parenthesis (`(see https://...)`), at a quote, at a
/// backtick and at an angle bracket: according to RFC 3986, `"`, `<`, `>` and `` ` `` cannot be
/// written unescaped in a URL, and `'` surrounds URLs in messages (`'https://...'`). Guzzle puts
/// the URL in backticks too: `` `POST https://...` resulted in ``.
///
/// File paths are found with three rules. Laravel adds the template's path to the end of view
/// errors: `... (View: C:\Users\İrem\Herd\shop\resources\views\storefront.blade.php)`. If the
/// path were not masked, the same error would be a separate pattern for every template and every
/// computer. The path of this suffix ends with `.php)`; even if it contains spaces
/// (`C:\Users\Ali Veli\...`), it is masked whole. Other paths: on Windows they start with a drive
/// letter (`C:\Users\...`, `C:/Users/...`). On Linux and macOS they start with `/` and end with a
/// file extension (`/var/www/shop/app/X.php`); the extension requirement leaves out URL paths
/// such as `/api/products/15`. The path must not go on after the extension: `/api/v1.2/products`
/// is not a path. A path starts after whitespace, a parenthesis, a quote, `=` or `,`; that
/// character (`${1}`) stays in place.
///
/// The domain of an email address ends with a part that starts with a letter (`.test`,
/// `.com.tr`): `laravel/framework@11.9.2` is a package version.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        rule(r#"https?://[^\s)"'<>`]+"#, "<URL>"),
        // `.*?`: lazy; it stops at the first `.php)`, so the suffixes of nested templates are
        // masked separately
        rule(r"\(View: .*?\.php\)", "(View: <PATH>)"),
        rule(
            r#"(^|[\s(\['"=,])[A-Za-z]:[\\/][^\s()'"<>|?*,;:]*"#,
            "${1}<PATH>",
        ),
        Rule {
            not_followed_by: Some(path_continues),
            ..rule(
                r#"(^|[\s(\['"=,])/(?:[^\s/()'"<>|?*,;:]+/)+[^\s/()'"<>|?*,;:]*\.[A-Za-z0-9]+"#,
                "${1}<PATH>",
            )
        },
        rule(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)*\.\p{L}[\w-]*", "<EMAIL>"),
        rule(
            r"(?-u:\b)[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}(?-u:\b)",
            "<UUID>",
        ),
        rule(
            r"(?-u:\b)[0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9]{2}:[0-9]{2}:[0-9]{2}(?-u:\b)",
            "<TIME>",
        ),
        rule(r"(?-u:\b)(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?-u:\b)", "<IP>"),
        rule(r"(?-u:\b)[0-9]+(?:\.[0-9]+)*(?-u:\b)", "<NUM>"),
    ]
});

fn rule(pattern: &str, placeholder: &'static str) -> Rule {
    Rule {
        // The patterns are constant; if one does not compile, that is a programming error and the
        // tests catch it right away.
        pattern: Regex::new(pattern).expect("invalid regular expression"),
        placeholder,
        not_followed_by: None,
    }
}

/// Does the path go on after the extension: a folder separator or a letter that could be part of
/// the extension. The ASCII letters and digits of the extension were already consumed by the
/// pattern.
fn path_continues(letter: char) -> bool {
    matches!(letter, '/' | '\\' | '_') || letter.is_alphanumeric()
}

/// Replaces the variable parts of the message with placeholders.
///
/// If no rule matches, the message is not copied; it is returned as it is, borrowed
/// (`Cow::Borrowed`).
pub fn mask(message: &str) -> Cow<'_, str> {
    let mut result = Cow::Borrowed(message);
    for rule in RULES.iter() {
        let replaced = match rule.not_followed_by {
            None => rule.pattern.replace_all(&result, rule.placeholder),
            Some(forbidden) => {
                let text: &str = &result;
                rule.pattern.replace_all(text, |captures: &Captures<'_>| {
                    let matched = captures.get(0).expect("group 0 is the whole match");
                    let mut replacement = String::new();
                    if text[matched.end()..].chars().next().is_some_and(forbidden) {
                        replacement.push_str(matched.as_str());
                    } else {
                        captures.expand(rule.placeholder, &mut replacement);
                    }
                    replacement
                })
            }
        };
        if let Cow::Owned(replaced) = replaced {
            result = Cow::Owned(replaced);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_masked() {
        assert_eq!(mask("User 381 logged in"), "User <NUM> logged in");
        assert_eq!(mask("Job finished (2.41 s)"), "Job finished (<NUM> s)");
        assert_eq!(
            mask("Application version 2.4.0 deployed"),
            "Application version <NUM> deployed"
        );
    }

    #[test]
    fn digits_inside_a_word_are_not_masked() {
        assert_eq!(mask("oauth2 token refreshed"), "oauth2 token refreshed");
        // The word boundary is ASCII: a digit stuck to a non-ASCII letter is masked
        assert_eq!(mask("Café5 ready"), "Café<NUM> ready");
        assert_eq!(mask("4ème attempt"), "<NUM>ème attempt");
        assert_eq!(mask("Payment3 and 3d"), "Payment3 and 3d");
        // Only ASCII digits are numbers
        assert_eq!(mask("Number ٣٤ arrived"), "Number ٣٤ arrived");
    }

    #[test]
    fn ip_is_not_split_into_numbers() {
        assert_eq!(
            mask("Connection refused [tcp://10.0.3.12:6379]"),
            "Connection refused [tcp://<IP>:<NUM>]"
        );
    }

    #[test]
    fn email_url_uuid_and_time_are_masked() {
        assert_eq!(
            mask("Duplicate entry 'ayse@example.test'"),
            "Duplicate entry '<EMAIL>'"
        );
        assert_eq!(
            mask(
                "(see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.payments.test/v1/charge"
            ),
            "(see <URL>) for <URL>"
        );
        // A URL in quotes: the closing quote is not part of the URL
        assert_eq!(
            mask(r#"The option "dsn" with value "https://key@o1.ingest.test/2" is invalid."#),
            r#"The option "dsn" with value "<URL>" is invalid."#
        );
        assert_eq!(
            mask("Webhook 'https://example.test/hook?x=1' did not respond"),
            "Webhook '<URL>' did not respond"
        );
        assert_eq!(mask("<https://example.test/a>"), "<<URL>>");
        // Guzzle puts the URL in backticks
        assert_eq!(
            mask(
                "Client error: `POST https://api.payments.test/v1/charge` resulted in a `404 Not Found` response"
            ),
            "Client error: `POST <URL>` resulted in a `<NUM> Not Found` response"
        );
        // The domain ends with a part that starts with a letter: a package version is not an
        // email address
        assert_eq!(
            mask("laravel/framework@11.9.2 installed"),
            "laravel/framework@<NUM> installed"
        );
        assert_eq!(mask("php@8.3"), "php@<NUM>");
        assert_eq!(
            mask("ayşe@örnek.com.tr and ali.veli+test@mail2.example.test"),
            "<EMAIL> and <EMAIL>"
        );
        assert_eq!(
            mask("Order 3f2b8c1e-9d4a-4e7b-8a6f-1c2d3e4f5a6b created"),
            "Order <UUID> created"
        );
        assert_eq!(
            mask("values (Ayşe, 2026-10-24 09:14:20)"),
            "values (Ayşe, <TIME>)"
        );
    }

    #[test]
    fn file_paths_are_masked() {
        // Windows: the template of a view error, include and "passed in ... on line"
        assert_eq!(
            mask(
                r"Route [campaign.list] not defined. (View: C:\Users\İrem\Herd\shop\resources\views\campaigns.blade.php)"
            ),
            "Route [campaign.list] not defined. (View: <PATH>)"
        );
        assert_eq!(
            mask(r"include(C:\Users\İrem\Herd\shop\public\header.php): Failed to open stream"),
            "include(<PATH>): Failed to open stream"
        );
        assert_eq!(
            mask(
                r"Too few arguments to function abort_if(), 1 passed in C:\Users\İrem\Herd\shop\app\Http\Middleware\IsAdmin.php on line 18 and at least 2 expected"
            ),
            "Too few arguments to function abort_if(), <NUM> passed in <PATH> on line <NUM> and at least <NUM> expected"
        );
        assert_eq!(
            mask("File 'C:/Users/İrem/Herd/shop/.env' could not be read"),
            "File '<PATH>' could not be read"
        );
        // A space in the user name: the path of the view suffix is still masked whole
        assert_eq!(
            mask(
                r"Undefined variable $product (View: C:\Users\Ali Veli\Herd\shop\resources\views\product.blade.php)"
            ),
            "Undefined variable $product (View: <PATH>)"
        );
        assert_eq!(
            mask("x (View: /home/ali veli/shop/a.blade.php) (View: /srv/b.blade.php)"),
            "x (View: <PATH>) (View: <PATH>)"
        );
        // A path can also be at the start of the message; with nested templates two paths come
        // one after the other
        assert_eq!(
            mask(r"C:\a\b.php (View: C:\a\c.blade.php) (View: C:\a\d.blade.php)"),
            "<PATH> (View: <PATH>) (View: <PATH>)"
        );
        // The line number is not part of the path
        assert_eq!(
            mask(r"Error at C:\shop\app\X.php:12 in handle"),
            "Error at <PATH>:<NUM> in handle"
        );
        // Linux and macOS: paths that end with an extension
        assert_eq!(
            mask(
                "Undefined variable $product (View: /var/www/shop/resources/views/storefront.blade.php)"
            ),
            "Undefined variable $product (View: <PATH>)"
        );
        assert_eq!(
            mask(
                r#"The stream or file "/var/www/shop/storage/logs/laravel.log" could not be opened"#
            ),
            r#"The stream or file "<PATH>" could not be opened"#
        );
        assert_eq!(
            mask("File /var/www/shop/app/X.php:12 failed"),
            "File <PATH>:<NUM> failed"
        );
        // Paths without an extension (URL paths, folders), relative paths and fractions are not
        // masked
        assert_eq!(
            mask("GET /api/products/15 slow"),
            "GET /api/products/<NUM> slow"
        );
        assert_eq!(
            mask("Folder /var/www/shop/storage is not writable"),
            "Folder /var/www/shop/storage is not writable"
        );
        assert_eq!(
            mask("include(header.php): Failed"),
            "include(header.php): Failed"
        );
        assert_eq!(mask("Step 1/2 done"), "Step <NUM>/<NUM> done");
        // If the path goes on after the extension, it is not a path
        assert_eq!(
            mask("GET /api/v1.2/products/15 slow"),
            "GET /api/v1.<NUM>/products/<NUM> slow"
        );
        assert_eq!(
            mask("Folder /srv/a.b/c is empty"),
            "Folder /srv/a.b/c is empty"
        );
        assert_eq!(mask("/srv/a.php_old"), "/srv/a.php_old");
        assert_eq!(mask("/srv/a.phpé"), "/srv/a.phpé");
        // Paths one after another; a full stop or a hyphen at the end of a sentence is not part
        // of the path
        assert_eq!(
            mask("/var/www/a.php /var/www/b.php,/srv/c.log"),
            "<PATH> <PATH>,<PATH>"
        );
        assert_eq!(
            mask("File /var/www/shop/app/X.php. Then /srv/a.log-old"),
            "File <PATH>. Then <PATH>-old"
        );
        // A letter at the end of a word is not mistaken for a drive; URLs are masked by their own
        // rule
        assert_eq!(mask(r"regex:\d+ is invalid"), r"regex:\d+ is invalid");
        assert_eq!(mask("See https://example.test/docs/guide.pdf"), "See <URL>");
    }

    #[test]
    fn nothing_is_copied_without_variables() {
        assert!(matches!(mask("Cache cleared"), Cow::Borrowed(_)));
        assert!(matches!(mask("User 381 logged in"), Cow::Owned(_)));
    }
}
