//! Property types. The frontmatter stays plain YAML; a property's type is worked out from its
//! name and value when a note opens (or chosen from its menu), and decides how it's edited.

use crate::vault::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropType {
    Text,
    Number,
    Checkbox,
    Date,
    List,
    Tags,
    Link,
    /// Nested YAML and block text: shown as written, edited in the file's text.
    Raw,
}

impl PropType {
    /// The types offered in menus (Raw only comes from the file).
    pub const CHOICES: [PropType; 7] = [PropType::Text, PropType::Number, PropType::Checkbox, PropType::Date, PropType::List, PropType::Tags, PropType::Link];

    pub fn label(self) -> &'static str {
        match self {
            PropType::Text => "Text",
            PropType::Number => "Number",
            PropType::Checkbox => "Checkbox",
            PropType::Date => "Date",
            PropType::List => "List",
            PropType::Tags => "Tags",
            PropType::Link => "Link",
            PropType::Raw => "As written",
        }
    }

    /// The type a property looks like.
    pub fn infer(key: &str, v: &Value) -> PropType {
        let tags = key.eq_ignore_ascii_case("tags") || key.eq_ignore_ascii_case("tag");
        match v {
            Value::Raw(_) => PropType::Raw,
            Value::List(_) if tags => PropType::Tags,
            Value::List(_) => PropType::List,
            Value::Text(_) if tags => PropType::Tags,
            Value::Text(t) => {
                let t = t.trim();
                if t.is_empty() {
                    let k = key.to_ascii_lowercase();
                    if ["created", "updated", "modified", "date", "due"].contains(&k.as_str()) { PropType::Date } else { PropType::Text }
                } else if parse_bool(t).is_some() {
                    PropType::Checkbox
                } else if parse_date(t).is_some() {
                    PropType::Date
                } else if t.parse::<f64>().is_ok_and(f64::is_finite) {
                    PropType::Number
                } else if is_url(t) {
                    PropType::Link
                } else {
                    PropType::Text
                }
            }
        }
    }

    /// A value of this type for a new property.
    pub fn empty(self) -> Value {
        match self {
            PropType::List | PropType::Tags => Value::List(Vec::new()),
            PropType::Checkbox => Value::Text("false".into()),
            PropType::Date => Value::Text(today()),
            _ => Value::Text(String::new()),
        }
    }

    /// `v` changed to this type, keeping what fits.
    pub fn convert(self, v: &Value) -> Value {
        let text = v.as_text();
        match self {
            PropType::List | PropType::Tags => match v {
                Value::List(l) => Value::List(l.clone()),
                _ if self == PropType::Tags => Value::List(text.split([',', '\n', ' ']).map(|s| s.trim().trim_start_matches('#').to_string()).filter(|s| !s.is_empty()).collect()),
                _ => Value::List(text.split([',', '\n']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()),
            },
            PropType::Checkbox => Value::Text(if parse_bool(&text) == Some(true) { "true" } else { "false" }.into()),
            PropType::Number => Value::Text(text.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map(|_| text.trim().to_string()).unwrap_or_default()),
            PropType::Date => Value::Text(if parse_date(&text).is_some() { text.trim().to_string() } else { today() }),
            PropType::Text | PropType::Link => Value::Text(text),
            PropType::Raw => v.clone(),
        }
    }
}

pub fn parse_bool(t: &str) -> Option<bool> {
    match t.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" => Some(true),
        "false" | "no" => Some(false),
        _ => None,
    }
}

/// `YYYY-MM-DD`, optionally followed by a time (`HH:MM`, a `T`, seconds…): the date part.
pub fn parse_date(t: &str) -> Option<chrono::NaiveDate> {
    let t = t.trim();
    let day = t.get(..10)?;
    let rest = &t[10..];
    if !(rest.is_empty() || rest.starts_with([' ', 'T'])) {
        return None;
    }
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

/// The same value with its date part replaced (any time after it is kept).
pub fn with_date(t: &str, d: chrono::NaiveDate) -> String {
    let t = t.trim();
    let rest = if parse_date(t).is_some() { &t[10..] } else { "" };
    format!("{}{rest}", d.format("%Y-%m-%d"))
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn is_url(t: &str) -> bool {
    let t = t.trim();
    (t.starts_with("https://") || t.starts_with("http://") || t.starts_with("mailto:")) && !t.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn infers_types() {
        assert_eq!(PropType::infer("tags", &Value::List(vec![])), PropType::Tags);
        assert_eq!(PropType::infer("aliases", &Value::List(vec![])), PropType::List);
        assert_eq!(PropType::infer("done", &t("true")), PropType::Checkbox);
        assert_eq!(PropType::infer("created", &t("2026-11-16 09:00")), PropType::Date);
        assert_eq!(PropType::infer("due", &t("")), PropType::Date);
        assert_eq!(PropType::infer("rating", &t("4.5")), PropType::Number);
        assert_eq!(PropType::infer("source", &t("https://example.org/a")), PropType::Link);
        assert_eq!(PropType::infer("title", &t("Hello")), PropType::Text);
        assert_eq!(PropType::infer("title", &t("2026-13-40")), PropType::Text);
        assert_eq!(PropType::infer("x", &Value::Raw(" |\n  a\n".into())), PropType::Raw);
    }

    #[test]
    fn converts_and_keeps_times() {
        assert_eq!(PropType::List.convert(&t("a, b,c")), Value::List(vec!["a".into(), "b".into(), "c".into()]));
        assert_eq!(PropType::Tags.convert(&t("#a b")), Value::List(vec!["a".into(), "b".into()]));
        assert_eq!(PropType::Text.convert(&Value::List(vec!["a".into(), "b".into()])), t("a, b"));
        assert_eq!(PropType::Checkbox.convert(&t("yes")), t("true"));
        assert_eq!(PropType::Number.convert(&t("abc")), t(""));
        let d = chrono::NaiveDate::from_ymd_opt(2027, 1, 5).unwrap();
        assert_eq!(with_date("2026-11-16 09:00", d), "2027-01-05 09:00");
        assert_eq!(with_date("", d), "2027-01-05");
    }
}
