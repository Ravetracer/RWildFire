// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Minimal XML tree parser, sufficient for `.flame` / `.ugr`-style documents.

#[derive(Clone, Debug, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// All descendants (depth first) with the given tag name.
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            } else {
                c.find_all(name, out);
            }
        }
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum XmlError {
    #[error("unexpected end of document")]
    Eof,
    #[error("malformed XML at byte {0}")]
    Malformed(usize),
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let ent = &rest[1..end];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if ent.starts_with("#x") => {
                if let Some(c) = u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            _ if ent.starts_with('#') => {
                if let Some(c) = ent[1..].parse::<u32>().ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            _ => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Parses a document and returns a synthetic root element containing the top-level elements.
pub fn parse(src: &str) -> Result<Element, XmlError> {
    let b = src.as_bytes();
    let mut pos = 0;
    let mut stack: Vec<Element> = vec![Element { name: "#root".into(), ..Default::default() }];
    while pos < b.len() {
        if b[pos] != b'<' {
            let end = src[pos..].find('<').map(|i| pos + i).unwrap_or(b.len());
            let text = &src[pos..end];
            if !text.trim().is_empty() {
                stack.last_mut().unwrap().text.push_str(&unescape(text));
            }
            pos = end;
            continue;
        }
        // Comments, declarations, processing instructions, CDATA
        if src[pos..].starts_with("<!--") {
            pos = src[pos..].find("-->").map(|i| pos + i + 3).ok_or(XmlError::Eof)?;
            continue;
        }
        if src[pos..].starts_with("<![CDATA[") {
            let end = src[pos..].find("]]>").map(|i| pos + i).ok_or(XmlError::Eof)?;
            stack.last_mut().unwrap().text.push_str(&src[pos + 9..end]);
            pos = end + 3;
            continue;
        }
        if src[pos..].starts_with("<?") || src[pos..].starts_with("<!") {
            pos = src[pos..].find('>').map(|i| pos + i + 1).ok_or(XmlError::Eof)?;
            continue;
        }
        if src[pos..].starts_with("</") {
            let end = src[pos..].find('>').map(|i| pos + i).ok_or(XmlError::Eof)?;
            let name = src[pos + 2..end].trim();
            pos = end + 1;
            // Pop until the matching element (tolerates sloppy nesting).
            if let Some(idx) = stack.iter().rposition(|e| e.name == name) {
                if idx == 0 {
                    continue;
                }
                while stack.len() > idx {
                    let el = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(el);
                }
            }
            continue;
        }
        // Start tag
        let start = pos;
        pos += 1;
        let name_start = pos;
        while pos < b.len() && !b[pos].is_ascii_whitespace() && b[pos] != b'>' && b[pos] != b'/' {
            pos += 1;
        }
        let mut el = Element { name: src[name_start..pos].to_string(), ..Default::default() };
        let mut self_closing = false;
        loop {
            while pos < b.len() && b[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos >= b.len() {
                return Err(XmlError::Eof);
            }
            if b[pos] == b'>' {
                pos += 1;
                break;
            }
            if b[pos] == b'/' {
                self_closing = true;
                pos += 1;
                continue;
            }
            let ks = pos;
            while pos < b.len() && b[pos] != b'=' && !b[pos].is_ascii_whitespace() && b[pos] != b'>' {
                pos += 1;
            }
            let key = src[ks..pos].to_string();
            while pos < b.len() && b[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos < b.len() && b[pos] == b'=' {
                pos += 1;
                while pos < b.len() && b[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos >= b.len() {
                    return Err(XmlError::Eof);
                }
                let q = b[pos];
                if q != b'"' && q != b'\'' {
                    return Err(XmlError::Malformed(start));
                }
                pos += 1;
                let vs = pos;
                while pos < b.len() && b[pos] != q {
                    pos += 1;
                }
                if pos >= b.len() {
                    return Err(XmlError::Eof);
                }
                el.attrs.push((key, unescape(&src[vs..pos])));
                pos += 1;
            } else if !key.is_empty() {
                el.attrs.push((key, String::new()));
            }
        }
        if self_closing {
            stack.last_mut().unwrap().children.push(el);
        } else {
            stack.push(el);
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(el);
    }
    Ok(stack.pop().unwrap())
}
