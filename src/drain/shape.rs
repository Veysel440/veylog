//! The shape of a token: does it come from code (a class name, a constant, an identifier, an
//! event name, a quoted key) or is it an ordinary word?

/// Is there at least one digit in it? "120ms", "a1b2", "S3" → yes; "<NUM>", "login" → no.
pub(super) fn has_digit(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_digit())
}

/// The shape of a token: is it a value that comes from code, or an ordinary word?
///
/// Ordinary words (`Payment`, `received`) consist only of letters. The values that come from code
/// (a class name, a constant, an identifier, an event name, a quoted key) have a recognizable
/// shape, and the fixed part of a message has no such token: if two messages differ only in two
/// code-like values of the same shape, that position is categorical. Short uppercase words (`GET`,
/// `OK`, `WARNING`) are cautiously counted as ordinary words: `GET /api` and `POST /api` are
/// different events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// A namespaced PHP class name: `App\Jobs\SendInvoice`, `\App\Models\Product`, JSON-escaped
    /// `App\\Jobs\\X`
    Class,
    /// An uppercase constant with at least one underscore: `ORDER_CREATED`
    Constant,
    /// camelCase or PascalCase (`SendInvoice`), snake_case (`send_invoice`), a PHP variable
    /// (`$product`)
    Identifier,
    /// A name split by dots or colons: `payment.failed`, `rates:update`, `example.test`
    Dotted,
    /// A single word wrapped in single or double quotes: `'price'`, `"dsn"`. SQL's backtick-quoted
    /// table and column names (`` `products` ``) do not count: if two queries differ by their
    /// table, they are different queries.
    Quoted,
    /// An ordinary word made only of letters: `Payment`, `received`, `GET`
    Word,
    /// Everything else: placeholders (`<NUM>`), punctuation, tokens that mix letters and digits
    /// (`oauth2`), member calls (`X::handle()`)
    Other,
}

impl Shape {
    /// Is it a value that comes from code? Ordinary words and unclear tokens are not.
    pub(super) fn is_code(self) -> bool {
        !matches!(self, Shape::Word | Shape::Other)
    }
}

/// The token with the parts that come from the sentence stripped: the opening brackets at the
/// start (`([{`) and the closing brackets and punctuation at the end (`)]},;:.`) are not part of
/// the token. `[App\Jobs\X]` → `App\Jobs\X`, `(Ayşe,` → `Ayşe`. If nothing is left after
/// stripping (`,`), the token itself.
pub fn token_core(token: &str) -> &str {
    let stripped = token
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches([')', ']', '}', ',', ';', ':', '.']);
    if stripped.is_empty() { token } else { stripped }
}

/// The shape of the token, looked at with the brackets and punctuation that come from the
/// sentence stripped (`token_core`): `[App\Jobs\X]` is a class name, `ORDER_CREATED:` a constant.
pub(super) fn token_shape(token: &str) -> Shape {
    let stripped = token_core(token);
    if let Some(inner) = quoted_inner(stripped) {
        // If there is no quote inside the quotes, it is a single quoted word
        return if !inner.is_empty() && !inner.contains(['\'', '"']) {
            Shape::Quoted
        } else {
            Shape::Other
        };
    }
    // A quote stuck to the token can come from the sentence: `'Ali` and `Veli'` are a quoted value
    // of two words
    let stripped = stripped
        .trim_start_matches(['\'', '"'])
        .trim_end_matches(['\'', '"']);
    if stripped.is_empty() {
        return Shape::Other;
    }
    if stripped.contains('\\') {
        return if is_class_name(stripped) {
            Shape::Class
        } else {
            Shape::Other
        };
    }
    if let Some(name) = stripped.strip_prefix('$') {
        return if is_identifier(name) {
            Shape::Identifier
        } else {
            Shape::Other
        };
    }
    if stripped.chars().all(char::is_alphabetic) {
        return if is_camel_case(stripped) {
            Shape::Identifier
        } else {
            Shape::Word
        };
    }
    if is_identifier(stripped) {
        // Letters and digits with at least one underscore (without an underscore it would have
        // counted as an ordinary word above, or would have a digit)
        return if stripped.contains('_') {
            if stripped
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase)
            {
                Shape::Constant
            } else {
                Shape::Identifier
            }
        } else if is_camel_case(stripped) {
            Shape::Identifier
        } else {
            Shape::Other
        };
    }
    if stripped.contains(['.', ':']) {
        let mut parts = stripped.split(['.', ':']);
        if parts.all(is_identifier) {
            return Shape::Dotted;
        }
    }
    Shape::Other
}

/// If the token starts and ends with the same quote (`'` or `"`), what is inside the quotes.
fn quoted_inner(token: &str) -> Option<&str> {
    let first = token.chars().next()?;
    if !matches!(first, '\'' | '"') || token.len() < 2 || !token.ends_with(first) {
        return None;
    }
    Some(&token[1..token.len() - 1])
}

/// Is it a name made of letters, digits and underscores, with at least one letter?
fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().any(char::is_alphabetic)
}

/// Does an uppercase letter come right after a lowercase letter? `SendInvoice`, `rateLimit` yes;
/// `Payment`, `SQLSTATE` no.
fn is_camel_case(name: &str) -> bool {
    name.chars()
        .zip(name.chars().skip(1))
        .any(|(previous, next)| previous.is_lowercase() && next.is_uppercase())
}

/// At least two ASCII identifiers separated by backslashes (by two backslashes in JSON)? There can
/// be a backslash at the start. `App\Jobs\X` yes; `resources\views\x.blade.php` (with an
/// extension, a path) and `App\Jobs\X::handle()` no.
fn is_class_name(name: &str) -> bool {
    let parts: Vec<&str> = name
        .strip_prefix('\\')
        .unwrap_or(name)
        .split('\\')
        .filter(|part| !part.is_empty())
        .collect();
    parts.len() >= 2
        && parts.iter().all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}
