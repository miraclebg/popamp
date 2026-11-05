//! Tolerant, order-preserving INI reader/writer.
//!
//! Classic skins ship hand-written `pledit.txt`, `region.txt` and the like, so
//! the parser accepts `;`, `#` and `//` comments, blank lines, keys outside any
//! section and odd spacing. Lookups of sections and keys are case-insensitive.

use std::fmt::Write as _;

#[derive(Debug, Default, Clone)]
pub struct Ini {
    sections: Vec<Section>,
}

#[derive(Debug, Clone)]
struct Section {
    name: String,
    entries: Vec<(String, String)>,
}

impl Ini {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut current = String::new();
        for raw in text.lines() {
            let line = raw.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(end) = rest.find(']') {
                    current = rest[..end].trim().to_string();
                    ini.section_mut(&current);
                    continue;
                }
            }
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                let value = line[eq + 1..].trim();
                if !key.is_empty() {
                    ini.set(&current, key, value);
                }
            }
        }
        ini
    }

    /// Parses bytes that may be UTF-8 or a legacy single-byte encoding.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        Self::parse(&crate::text::decode_lossy(bytes))
    }

    fn section_mut(&mut self, name: &str) -> &mut Section {
        let idx = match self.sections.iter().position(|s| s.name.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                self.sections.push(Section { name: name.to_string(), entries: Vec::new() });
                self.sections.len() - 1
            }
        };
        &mut self.sections[idx]
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case(section))
            .flat_map(|s| s.entries.iter())
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn has_section(&self, section: &str) -> bool {
        self.sections.iter().any(|s| s.name.eq_ignore_ascii_case(section))
    }

    pub fn set(&mut self, section: &str, key: &str, value: impl Into<String>) {
        let value = value.into();
        let sec = self.section_mut(section);
        match sec.entries.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(entry) => entry.1 = value,
            None => sec.entries.push((key.to_string(), value)),
        }
    }

    /// Removes `key` from every section named `section`, returning its value.
    pub fn remove(&mut self, section: &str, key: &str) -> Option<String> {
        let mut removed = None;
        for sec in self.sections.iter_mut().filter(|s| s.name.eq_ignore_ascii_case(section)) {
            if let Some(i) = sec.entries.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)) {
                removed = removed.or(Some(sec.entries.remove(i).1));
            }
        }
        removed
    }

    pub fn to_string_pretty(&self) -> String {
        let mut out = String::new();
        for sec in &self.sections {
            if !sec.name.is_empty() {
                let _ = writeln!(out, "[{}]", sec.name);
            }
            for (k, v) in &sec.entries {
                let _ = writeln!(out, "{k}={v}");
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tolerantly() {
        let ini = Ini::parse("\u{feff}[Text]\r\nNormal = #00FF00\n// comment\n;x\nFONT=Arial\n[text]\nCurrent=#FFFFFF\n");
        assert_eq!(ini.get("TEXT", "normal"), Some("#00FF00"));
        assert_eq!(ini.get("text", "Font"), Some("Arial"));
        assert_eq!(ini.get("Text", "current"), Some("#FFFFFF"));
        assert_eq!(ini.get("Text", "missing"), None);
    }

    #[test]
    fn round_trips() {
        let mut ini = Ini::new();
        ini.set("Popamp", "volume", "200");
        ini.set("Popamp", "VOLUME", "100");
        let back = Ini::parse(&ini.to_string_pretty());
        assert_eq!(back.get("popamp", "volume"), Some("100"));
    }
}
