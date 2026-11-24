//! The iCalendar text format (RFC 5545), read and written without losing anything: a file is
//! components (`BEGIN:VEVENT` … `END:VEVENT`) holding properties (`NAME;PARAM=x:value`) and
//! other components. What Rivet doesn't use is kept as it was and written back unchanged.

/// One property: `DTSTART;TZID=Europe/Paris:20261124T090000`.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// In capitals.
    pub name: String,
    /// Each parameter's name (in capitals) and value (without quotes).
    pub params: Vec<(String, String)>,
    /// As written, still escaped for text values (see [`unescape`]).
    pub value: String,
}

impl Prop {
    pub fn new(name: &str, value: impl Into<String>) -> Prop {
        Prop { name: name.to_ascii_uppercase(), params: Vec::new(), value: value.into() }
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn with_param(mut self, name: &str, value: &str) -> Prop {
        self.params.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        self.params.push((name.to_ascii_uppercase(), value.to_string()));
        self
    }

    /// The value of a text property, unescaped.
    pub fn text(&self) -> String {
        unescape(&self.value)
    }
}

/// A component: `VCALENDAR`, `VEVENT`, `VTODO`, `VALARM`, `VTIMEZONE`…
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    pub name: String,
    pub props: Vec<Prop>,
    pub subs: Vec<Component>,
}

impl Component {
    pub fn new(name: &str) -> Component {
        Component { name: name.to_ascii_uppercase(), ..Default::default() }
    }

    /// The first property of this name.
    pub fn prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Every property of this name, in order.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Prop> {
        self.props.iter().filter(move |p| p.name.eq_ignore_ascii_case(name))
    }

    /// A text property's value, unescaped (empty when it has none).
    pub fn text(&self, name: &str) -> String {
        self.prop(name).map(Prop::text).unwrap_or_default()
    }

    /// Put `prop` in place of every property of its name (where the first was, else at the end).
    pub fn set(&mut self, prop: Prop) {
        match self.props.iter().position(|p| p.name == prop.name) {
            Some(i) => {
                self.props[i] = prop.clone();
                let name = prop.name;
                let mut seen = false;
                self.props.retain(|p| {
                    if p.name != name {
                        return true;
                    }
                    let keep = !seen;
                    seen = true;
                    keep
                });
            }
            None => self.props.push(prop),
        }
    }

    /// Set a text property (escaped), or take it out when `value` is empty.
    pub fn set_text(&mut self, name: &str, value: &str) {
        if value.is_empty() {
            self.remove(name);
        } else {
            self.set(Prop::new(name, escape(value)));
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.props.retain(|p| !p.name.eq_ignore_ascii_case(name));
    }

    /// The components of this name inside it.
    pub fn subs<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Component> {
        self.subs.iter().filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    /// As text, with CRLF line ends and long lines folded, as the format asks.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out);
        out
    }

    fn write_into(&self, out: &mut String) {
        fold_into(out, &format!("BEGIN:{}", self.name));
        for p in &self.props {
            let mut line = p.name.clone();
            for (k, v) in &p.params {
                let quote = v.contains([':', ';', ',']);
                line.push_str(&if quote { format!(";{k}=\"{v}\"") } else { format!(";{k}={v}") });
            }
            line.push(':');
            line.push_str(&p.value);
            fold_into(out, &line);
        }
        for c in &self.subs {
            c.write_into(out);
        }
        fold_into(out, &format!("END:{}", self.name));
    }
}

/// A line, folded at 75 bytes (never inside a character), ended with CRLF.
fn fold_into(out: &mut String, line: &str) {
    let mut used = 0;
    for ch in line.chars() {
        let n = ch.len_utf8();
        if used + n > 75 {
            out.push_str("\r\n ");
            used = 1;
        }
        out.push(ch);
        used += n;
    }
    out.push_str("\r\n");
}

/// The components in a file's text (usually one `VCALENDAR`). Lines that can't be read are
/// skipped rather than failing the whole file; components left open at the end are closed.
pub fn parse(text: &str) -> Vec<Component> {
    let mut top: Vec<Component> = Vec::new();
    let mut stack: Vec<Component> = Vec::new();
    for line in unfold(text) {
        let Some(p) = parse_line(&line) else { continue };
        match p.name.as_str() {
            "BEGIN" => stack.push(Component::new(&p.value)),
            "END" => {
                if let Some(c) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.subs.push(c),
                        None => top.push(c),
                    }
                }
            }
            _ => {
                if let Some(c) = stack.last_mut() {
                    c.props.push(p);
                }
            }
        }
    }
    while let Some(c) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.subs.push(c),
            None => top.push(c),
        }
    }
    top
}

/// The logical lines: a line starting with a space or tab goes on the one before.
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match raw.strip_prefix([' ', '\t']) {
            Some(rest) if !out.is_empty() => out.last_mut().unwrap().push_str(rest),
            _ if raw.is_empty() => {}
            _ => out.push(raw.to_string()),
        }
    }
    out
}

/// `NAME;PARAM=a;PARAM="b:c":value`, the colon that ends the name being the first outside quotes.
fn parse_line(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let mut split = None;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let i = split?;
    let (head, value) = (&line[..i], &line[i + 1..]);
    let mut parts = split_outside_quotes(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()))
        })
        .collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut quoted, mut start) = (false, 0);
    for (i, ch) in s.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == sep && !quoted {
            out.push(&s[start..i]);
            start = i + 1;
        }
    }
    out.push(&s[start..]);
    out
}

/// A text value as written: backslashes, semicolons, commas and line breaks escaped.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// A text value as meant.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(c) => out.push(c),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_folded_lines_params_and_nested_parts() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a@b\r\nSUMMARY:Lunch\\, then a\r\n  walk\r\nDTSTART;TZID=\"Europe/Paris\":20261124T120000\r\nBEGIN:VALARM\r\nTRIGGER:-PT10M\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let top = parse(text);
        assert_eq!(top.len(), 1);
        let ev = top[0].subs("VEVENT").next().unwrap();
        assert_eq!(ev.text("SUMMARY"), "Lunch, then a walk");
        assert_eq!(ev.prop("DTSTART").unwrap().param("tzid"), Some("Europe/Paris"));
        assert_eq!(ev.subs("VALARM").count(), 1);
    }

    #[test]
    fn writes_what_it_read() {
        let mut c = Component::new("VEVENT");
        c.set_text("SUMMARY", "Line one\nline two; with, marks \\ and a long tail that keeps going past the edge of a line");
        c.set(Prop::new("DTSTART", "20261124T090000").with_param("TZID", "America/New_York"));
        let text = c.write();
        assert!(text.lines().all(|l| l.trim_end_matches('\r').len() <= 75));
        let back = parse(&text).remove(0);
        assert_eq!(back, c);
        assert_eq!(back.text("SUMMARY"), "Line one\nline two; with, marks \\ and a long tail that keeps going past the edge of a line");
    }

    #[test]
    fn set_replaces_every_copy_in_place() {
        let mut c = Component::new("VEVENT");
        c.props = vec![Prop::new("A", "1"), Prop::new("X", "1"), Prop::new("A", "2")];
        c.set(Prop::new("A", "3"));
        assert_eq!(c.props, vec![Prop::new("A", "3"), Prop::new("X", "1")]);
    }
}
