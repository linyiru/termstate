use core::fmt;

/// A field that a limit or validation error refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// A key name (any key).
    Key,
    /// `id`
    Id,
    /// `app`
    App,
    /// `title`
    Title,
    /// `msg`
    Msg,
    /// `progress`
    Progress,
}

impl Field {
    /// The key name as it appears on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            Field::Key => "key",
            Field::Id => "id",
            Field::App => "app",
            Field::Title => "title",
            Field::Msg => "msg",
            Field::Progress => "progress",
        }
    }
}

/// Why a report was discarded (terminal side) or refused (program side).
///
/// Every variant means the same thing to a terminal: nothing from the report
/// is applied. The distinction exists for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The whole sequence exceeds [`MAX_SEQUENCE`](crate::limits::MAX_SEQUENCE).
    SequenceTooLong,
    /// A field exceeds one of the limits.
    TooLong(Field),
    /// `msg` or `title` is not valid base64.
    InvalidBase64(Field),
    /// `msg` or `title` decodes to bytes that are not UTF-8.
    InvalidUtf8(Field),
    /// `msg` or `title` contains a control character (U+0000–U+001F, U+007F, U+0080–U+009F).
    ControlCharacter(Field),
    /// The report has no `state`.
    MissingState,
    /// The `state` value is not one this implementation recognizes.
    UnknownState,
    /// `id` does not match the id grammar.
    InvalidId,
    /// `app` does not match `[A-Za-z0-9_.+-]{1,32}` (program side only;
    /// terminals treat an invalid `app` as absent).
    InvalidApp,
    /// `progress` is outside 0..=100 (program side only).
    InvalidProgress,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SequenceTooLong => f.write_str("sequence exceeds 4096 bytes"),
            Error::TooLong(field) => write!(f, "{} exceeds its length limit", field.as_str()),
            Error::InvalidBase64(field) => write!(f, "{} is not valid base64", field.as_str()),
            Error::InvalidUtf8(field) => write!(f, "{} is not valid UTF-8", field.as_str()),
            Error::ControlCharacter(field) => {
                write!(f, "{} contains a control character", field.as_str())
            }
            Error::MissingState => f.write_str("report has no state"),
            Error::UnknownState => f.write_str("unrecognized state"),
            Error::InvalidId => f.write_str("id does not match the id grammar"),
            Error::InvalidApp => f.write_str("app does not match [A-Za-z0-9_.+-]{1,32}"),
            Error::InvalidProgress => f.write_str("progress is not an integer 0 to 100"),
        }
    }
}

impl core::error::Error for Error {}
