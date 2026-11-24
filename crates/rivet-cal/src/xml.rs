//! Just enough XML for WebDAV's answers: elements by their local name (the namespace prefix is
//! dropped, as `D:href`, `d:href` and `href` all mean the same thing from one server to the
//! next), their text with entities and CDATA read, and their children. Attributes, comments and
//! declarations are skipped.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// Without its prefix, in lower case.
    pub name: String,
    pub text: String,
    /// Its attributes, by local name.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// The first child of this name.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Every element of this name inside it, at any depth.
    pub fn find<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.find(name, out);
        }
    }

    pub fn all<'a>(&'a self, name: &str) -> Vec<&'a Node> {
        let mut out = Vec::new();
        self.find(name, &mut out);
        out
    }

    /// The text of the first element of this name inside it, at any depth.
    pub fn text_of(&self, name: &str) -> Option<String> {
        self.all(name).first().map(|n| n.text.trim().to_string())
    }
}

/// `a="1" b='2'` after a tag's name.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag.split_once(char::is_whitespace).map(|(_, r)| r).unwrap_or("");
    while let Some(eq) = rest.find('=') {
        let key = local(rest[..eq].trim());
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(end) = after[1..].find(q) else { break };
        out.push((key, unescape(&after[1..1 + end])));
        rest = &after[end + 2..];
    }
    out
}

fn local(tag: &str) -> String {
    let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

/// `&lt;` and the rest, and `&#…;`.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|e| *e < 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..end];
        let ch = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Text for putting in XML.
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The document's top element (an empty one when there's none).
pub fn parse(text: &str) -> Node {
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut rest = text;
    while !rest.is_empty() {
        let Some(i) = rest.find('<') else {
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&unescape(rest));
            }
            break;
        };
        if i > 0
            && let Some(top) = stack.last_mut()
        {
            top.text.push_str(&unescape(&rest[..i]));
        }
        let tail = &rest[i..];
        if let Some(body) = tail.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&body[..end]);
            }
            rest = body.get(end + 3..).unwrap_or("");
            continue;
        }
        if let Some(body) = tail.strip_prefix("<!--") {
            rest = body.find("-->").map(|e| &body[e + 3..]).unwrap_or("");
            continue;
        }
        let Some(close) = tail.find('>') else { break };
        let tag = &tail[1..close];
        rest = &tail[close + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            let name = local(name);
            // Close up to the element this ends (a missing end tag doesn't lose the rest).
            while stack.len() > 1 {
                let done = stack.pop().unwrap_or_default();
                let same = done.name == name;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(done);
                }
                if same {
                    break;
                }
            }
            continue;
        }
        let node = Node { name: local(tag), attrs: attributes(tag.trim_end_matches('/')), ..Default::default() };
        if tag.ends_with('/') {
            if let Some(top) = stack.last_mut() {
                top.children.push(node);
            }
        } else {
            stack.push(node);
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.children.push(done);
        }
    }
    let root = stack.pop().unwrap_or_default();
    root.children.into_iter().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_multistatus() {
        let x = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
 <d:response><d:href>/cal/a.ics</d:href><d:propstat><d:prop><d:getetag>"1&amp;2"</d:getetag>
 <cal:calendar-data><![CDATA[BEGIN:VCALENDAR
END:VCALENDAR]]></cal:calendar-data><d:resourcetype/></d:prop></d:propstat></d:response>
 <D:response><D:href>/cal/b.ics</D:href><!-- a note --><D:propstat><D:prop><D:getetag>"3"</D:getetag><C:calendar-data>BEGIN:VCALENDAR&#13;
END:VCALENDAR</C:calendar-data></D:prop></D:propstat></D:response>
</d:multistatus>"#;
        let root = parse(x);
        assert_eq!(root.name, "multistatus");
        let rs = root.all("response");
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].text_of("href").as_deref(), Some("/cal/a.ics"));
        assert_eq!(rs[0].text_of("getetag").as_deref(), Some("\"1&2\""));
        assert!(rs[0].text_of("calendar-data").unwrap().starts_with("BEGIN:VCALENDAR"));
        assert!(rs[1].text_of("calendar-data").unwrap().contains("\r\nEND"));
        assert!(rs[0].child("propstat").unwrap().all("resourcetype").len() == 1);
        let comps = parse(r#"<C:supported-calendar-component-set xmlns:C="x"><C:comp name="VEVENT"/><C:comp name='VTODO' /></C:supported-calendar-component-set>"#);
        assert_eq!(comps.all("comp").iter().filter_map(|c| c.attr("name")).collect::<Vec<_>>(), ["VEVENT", "VTODO"]);
    }
}
