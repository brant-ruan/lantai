//! The fixed sets of values stored in text columns, as enums. Each converts to and from its
//! stored text (and to and from SQL), so a misspelled state cannot compile, and a value this
//! build does not know fails when read instead of being treated as some default.
//!
//! The stored text never changes: it is in users' databases. SQL fragments that compare
//! against a value spell it out (`sql.rs`); the test below keeps them in step.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;

/// Defines an enum whose variants are stored as the given strings.
macro_rules! stored_enum {
    (
        $(#[$meta:meta])*
        $name:ident, $what:literal {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stored text.
            pub fn as_str(self) -> &'static str {
                match self {
                    $( $name::$variant => $text, )+
                }
            }

            /// The value stored as `s`; `None` for text this build does not know.
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $( $text => Some($name::$variant), )+
                    _ => None,
                }
            }

            /// Like [`Self::parse`], with an error that names what was expected.
            pub fn parse_or_err(s: &str) -> anyhow::Result<Self> {
                Self::parse(s).ok_or_else(|| {
                    let known: Vec<&str> = Self::ALL.iter().map(|v| v.as_str()).collect();
                    anyhow::anyhow!(concat!("invalid ", $what, " {:?}; expected one of {}"), s, known.join(", "))
                })
            }
        }

        impl std::fmt::Display for $name {
            /// The stored text, honouring width and alignment (`{:<10}`) as a `&str` would.
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.pad(self.as_str())
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let s = value.as_str()?;
                $name::parse(s).ok_or_else(|| {
                    FromSqlError::Other(format!(concat!("unknown ", $what, " {:?}"), s).into())
                })
            }
        }
    };
}

stored_enum! {
    /// Where a project note shows in its project's section: before or after the generated points.
    #[derive(Default)]
    HumanPosition, "note position" {
        Top => "top",
        #[default]
        Bottom => "bottom",
    }
}

stored_enum! {
    /// A slice's own say in whether it is reported, over its session's.
    #[derive(Default)]
    IgnoreState, "slice ignore state" {
        /// Follow the session.
        #[default]
        Inherit => "inherit",
        Ignored => "ignored",
        Included => "included",
    }
}

stored_enum! {
    /// How a session got its project.
    #[derive(Default)]
    AssignmentSource, "assignment source" {
        #[default]
        Unassigned => "unassigned",
        /// A project rule matched; re-evaluating rules may change or clear it.
        Rule => "rule",
        /// The user chose it (or accepted a suggestion); rules never override it.
        Manual => "manual",
    }
}

stored_enum! {
    /// Where a pending project suggestion for a session came from.
    SuggestionSource, "suggestion source" {
        Rule => "rule",
        /// A model judged it while digesting.
        Llm => "llm",
    }
}

stored_enum! {
    /// Whether a report is still being worked on.
    #[derive(Default)]
    ReportStatus, "report status" {
        #[default]
        Draft => "draft",
        Final => "final",
    }
}

stored_enum! {
    /// Who wrote a report version.
    ReportOrigin, "report origin" {
        Generated => "generated",
        /// Saved by hand in the editor.
        UserEdit => "user_edit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_and_unknown_text_is_rejected() {
        fn round_trip<T: Copy + PartialEq + std::fmt::Debug>(
            all: &[T],
            text: fn(T) -> &'static str,
            parse: fn(&str) -> Option<T>,
        ) {
            for v in all {
                assert_eq!(parse(text(*v)), Some(*v));
            }
        }
        round_trip(
            HumanPosition::ALL,
            HumanPosition::as_str,
            HumanPosition::parse,
        );
        round_trip(IgnoreState::ALL, IgnoreState::as_str, IgnoreState::parse);
        round_trip(
            AssignmentSource::ALL,
            AssignmentSource::as_str,
            AssignmentSource::parse,
        );
        round_trip(
            SuggestionSource::ALL,
            SuggestionSource::as_str,
            SuggestionSource::parse,
        );
        round_trip(ReportStatus::ALL, ReportStatus::as_str, ReportStatus::parse);
        round_trip(ReportOrigin::ALL, ReportOrigin::as_str, ReportOrigin::parse);
        assert_eq!(IgnoreState::parse("Ignored"), None);
        let err = IgnoreState::parse_or_err("x").unwrap_err().to_string();
        assert_eq!(
            err,
            "invalid slice ignore state \"x\"; expected one of inherit, ignored, included"
        );
    }

    #[test]
    fn display_pads_like_the_stored_text() {
        // CLI tables align columns with `{:<n}`; a value must take the width its text would.
        assert_eq!(format!("{:<10}|", ReportOrigin::Generated), "generated |");
        assert_eq!(format!("{:>6}|", ReportStatus::Final), " final|");
    }

    #[test]
    fn sql_fragments_spell_the_stored_values() {
        let quoted = |v: &dyn std::fmt::Display| format!("'{v}'");
        assert!(crate::sql::SLICE_INCLUDED.contains(&quoted(&IgnoreState::Included)));
        assert!(crate::sql::SLICE_INCLUDED.contains(&quoted(&IgnoreState::Inherit)));
        assert!(crate::sql::SLICE_UNASSIGNED.contains(&quoted(&AssignmentSource::Manual)));
    }

    #[test]
    fn an_unknown_stored_value_fails_to_read() {
        let store = crate::Store::open_in_memory().unwrap();
        let read = |text: &str| -> rusqlite::Result<ReportStatus> {
            store.conn.query_row("SELECT ?1", [text], |r| r.get(0))
        };
        assert_eq!(read("final").unwrap(), ReportStatus::Final);
        assert!(read("Final").is_err());
    }
}
