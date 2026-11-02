//! Advanced Title Formatting.
//!
//! `%field%` inserts a tag, `$func(a,b)` calls a function, `[...]` is emitted
//! only when a field inside it resolved to something, `'...'` quotes literal
//! text. Every value carries a truth flag: fields are true when non-empty,
//! literals are false, and functions decide for themselves.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Supplies field values for a track.
pub trait Fields {
    /// `name` is lower-case. Return `None` (or an empty string) if unknown.
    fn field(&self, name: &str) -> Option<String>;
}

impl<F: Fn(&str) -> Option<String>> Fields for F {
    fn field(&self, name: &str) -> Option<String> {
        self(name)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    Field(String),
    Func(String, Vec<Vec<Node>>),
    Cond(Vec<Node>),
}

#[derive(Debug, Clone)]
pub struct Format {
    nodes: Vec<Node>,
}

impl Format {
    pub fn parse(src: &str) -> Format {
        let chars: Vec<char> = src.chars().collect();
        let mut p = Parser { chars: &chars, pos: 0 };
        let nodes = p.seq(&[]);
        Format { nodes }
    }

    /// Parses through a process-wide cache.
    pub fn cached(src: &str) -> Arc<Format> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<Format>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 64 {
            map.clear();
        }
        map.entry(src.to_string()).or_insert_with(|| Arc::new(Format::parse(src))).clone()
    }

    pub fn eval(&self, fields: &dyn Fields) -> String {
        eval_seq(&self.nodes, fields).text
    }
}

/// Formats `src` for `fields` in one call.
pub fn format(src: &str, fields: &dyn Fields) -> String {
    Format::cached(src).eval(fields)
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Parses until one of `stops` (not consumed) or end of input.
    fn seq(&mut self, stops: &[char]) -> Vec<Node> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        let flush = |text: &mut String, nodes: &mut Vec<Node>| {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(text)));
            }
        };
        while let Some(c) = self.peek() {
            if stops.contains(&c) {
                break;
            }
            self.pos += 1;
            match c {
                '%' => {
                    if self.peek() == Some('%') {
                        self.pos += 1;
                        text.push('%');
                        continue;
                    }
                    let start = self.pos;
                    while self.peek().is_some_and(|c| c != '%') {
                        self.pos += 1;
                    }
                    if self.peek() == Some('%') {
                        let name: String = self.chars[start..self.pos].iter().collect();
                        self.pos += 1;
                        flush(&mut text, &mut nodes);
                        nodes.push(Node::Field(name.to_lowercase()));
                    } else {
                        // Unterminated: keep it as text.
                        text.push('%');
                        text.extend(&self.chars[start..self.pos]);
                    }
                }
                '$' => {
                    if self.peek() == Some('$') {
                        self.pos += 1;
                        text.push('$');
                        continue;
                    }
                    let start = self.pos;
                    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                        self.pos += 1;
                    }
                    let name: String = self.chars[start..self.pos].iter().collect();
                    if self.peek() != Some('(') || name.is_empty() {
                        text.push('$');
                        text.push_str(&name);
                        continue;
                    }
                    self.pos += 1; // '('
                    let mut args = Vec::new();
                    loop {
                        args.push(self.seq(&[',', ')']));
                        match self.peek() {
                            Some(',') => self.pos += 1,
                            Some(')') => {
                                self.pos += 1;
                                break;
                            }
                            _ => break, // unterminated: take what we have
                        }
                    }
                    if args.len() == 1 && args[0].is_empty() {
                        args.clear();
                    }
                    flush(&mut text, &mut nodes);
                    nodes.push(Node::Func(name.to_lowercase(), args));
                }
                '[' => {
                    if self.peek() == Some('[') {
                        self.pos += 1;
                        text.push('[');
                        continue;
                    }
                    let inner = self.seq(&[']']);
                    if self.peek() == Some(']') {
                        self.pos += 1;
                    }
                    flush(&mut text, &mut nodes);
                    nodes.push(Node::Cond(inner));
                }
                ']' if self.peek() == Some(']') => {
                    self.pos += 1;
                    text.push(']');
                }
                '\'' => {
                    if self.peek() == Some('\'') {
                        self.pos += 1;
                        text.push('\'');
                        continue;
                    }
                    while let Some(q) = self.peek() {
                        self.pos += 1;
                        if q == '\'' {
                            if self.peek() == Some('\'') {
                                self.pos += 1;
                                text.push('\'');
                                continue;
                            }
                            break;
                        }
                        text.push(q);
                    }
                }
                _ => text.push(c),
            }
        }
        flush(&mut text, &mut nodes);
        nodes
    }
}

#[derive(Debug, Clone, Default)]
struct Val {
    text: String,
    truth: bool,
}

impl Val {
    fn new(text: impl Into<String>, truth: bool) -> Self {
        Self { text: text.into(), truth }
    }
    fn num(&self) -> i64 {
        parse_leading_int(&self.text)
    }
}

fn parse_leading_int(s: &str) -> i64 {
    let s = s.trim();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let n: i64 = digits.chars().take_while(char::is_ascii_digit).take(18).fold(0, |acc, c| acc * 10 + (c as i64 - '0' as i64));
    if neg {
        -n
    } else {
        n
    }
}

fn eval_seq(nodes: &[Node], f: &dyn Fields) -> Val {
    let mut out = Val::default();
    for n in nodes {
        let v = eval_node(n, f);
        out.text.push_str(&v.text);
        out.truth |= v.truth;
    }
    out
}

fn eval_node(node: &Node, f: &dyn Fields) -> Val {
    match node {
        Node::Text(t) => Val::new(t.clone(), false),
        Node::Field(name) => match f.field(name).filter(|s| !s.is_empty()) {
            Some(v) => Val::new(v, true),
            None => Val::new("", false),
        },
        Node::Cond(inner) => {
            let v = eval_seq(inner, f);
            if v.truth {
                Val::new(v.text, true)
            } else {
                Val::default()
            }
        }
        Node::Func(name, args) => call(name, args, f),
    }
}

fn chars_left(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn chars_right(s: &str, n: usize) -> String {
    let len = s.chars().count();
    s.chars().skip(len.saturating_sub(n)).collect()
}

fn path_file(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn capitalize_words(s: &str, lower_rest: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else if lower_rest {
                out.extend(c.to_lowercase());
            } else {
                out.push(c);
            }
            start = false;
        } else {
            out.push(c);
            start = c.is_whitespace() || c == '(' || c == '[' || c == '-' || c == '/';
        }
    }
    out
}

fn call(name: &str, args: &[Vec<Node>], f: &dyn Fields) -> Val {
    // Lazy argument evaluation keeps $if() from evaluating the unused branch.
    let arg = |i: usize| args.get(i).map(|a| eval_seq(a, f)).unwrap_or_default();
    let all = || args.iter().map(|a| eval_seq(a, f)).collect::<Vec<_>>();
    let n = args.len();
    match name {
        "if" if n >= 2 => {
            if arg(0).truth {
                arg(1)
            } else {
                arg(2)
            }
        }
        "if2" if n >= 2 => {
            let a = arg(0);
            if a.truth {
                a
            } else {
                arg(1)
            }
        }
        "if3" if n >= 2 => {
            for i in 0..n - 1 {
                let v = arg(i);
                if v.truth {
                    return v;
                }
            }
            arg(n - 1)
        }
        "ifgreater" if n >= 4 => {
            if arg(0).num() > arg(1).num() {
                arg(2)
            } else {
                arg(3)
            }
        }
        "iflonger" if n >= 4 => {
            if arg(0).text.chars().count() as i64 > arg(1).num() {
                arg(2)
            } else {
                arg(3)
            }
        }
        "ifequal" if n >= 4 => {
            if arg(0).num() == arg(1).num() {
                arg(2)
            } else {
                arg(3)
            }
        }
        "select" if n >= 2 => {
            let idx = arg(0).num();
            if idx >= 1 && (idx as usize) < n {
                arg(idx as usize)
            } else {
                Val::default()
            }
        }
        "isempty" | "not" => Val::new("", !arg(0).truth && (name == "not" || arg(0).text.is_empty())),
        "and" => Val::new("", all().iter().all(|v| v.truth)),
        "or" => Val::new("", all().iter().any(|v| v.truth)),
        "xor" => Val::new("", all().iter().filter(|v| v.truth).count() % 2 == 1),
        "greater" => Val::new("", arg(0).num() > arg(1).num()),
        "strcmp" => Val::new("", arg(0).text == arg(1).text),
        "stricmp" => Val::new("", arg(0).text.to_lowercase() == arg(1).text.to_lowercase()),
        "len" => {
            let v = arg(0);
            Val::new(v.text.chars().count().to_string(), v.truth)
        }
        "left" | "cut" => {
            let v = arg(0);
            Val::new(chars_left(&v.text, arg(1).num().max(0) as usize), v.truth)
        }
        "right" => {
            let v = arg(0);
            Val::new(chars_right(&v.text, arg(1).num().max(0) as usize), v.truth)
        }
        "substr" => {
            let v = arg(0);
            let from = (arg(1).num().max(1) - 1) as usize;
            let to = if n >= 3 { arg(2).num().max(0) as usize } else { usize::MAX };
            let s: String = v.text.chars().skip(from).take(to.saturating_sub(from)).collect();
            Val::new(s, v.truth)
        }
        "pad" | "lpad" | "padcut" => {
            let v = arg(0);
            let width = arg(1).num().max(0) as usize;
            let fill = if n >= 3 { arg(2).text.chars().next().unwrap_or(' ') } else { ' ' };
            let mut s = if name == "padcut" { chars_left(&v.text, width) } else { v.text.clone() };
            let missing = width.saturating_sub(s.chars().count());
            let padding: String = std::iter::repeat_n(fill, missing).collect();
            if name == "lpad" {
                s = padding + &s;
            } else {
                s.push_str(&padding);
            }
            Val::new(s, v.truth)
        }
        "repeat" => {
            let v = arg(0);
            Val::new(v.text.repeat(arg(1).num().clamp(0, 1000) as usize), v.truth)
        }
        "replace" if n >= 3 => {
            let v = arg(0);
            let from = arg(1).text;
            if from.is_empty() {
                return v;
            }
            Val::new(v.text.replace(&from, &arg(2).text), v.truth)
        }
        "trim" => {
            let v = arg(0);
            Val::new(v.text.trim().to_string(), v.truth)
        }
        "lower" => {
            let v = arg(0);
            Val::new(v.text.to_lowercase(), v.truth)
        }
        "upper" => {
            let v = arg(0);
            Val::new(v.text.to_uppercase(), v.truth)
        }
        "caps" => {
            let v = arg(0);
            Val::new(capitalize_words(&v.text, true), v.truth)
        }
        "caps2" => {
            let v = arg(0);
            Val::new(capitalize_words(&v.text, false), v.truth)
        }
        "abbr" => {
            let v = arg(0);
            let s: String =
                v.text.split(|c: char| c.is_whitespace() || c == '-' || c == '(' || c == ')').filter_map(|w| w.chars().next()).collect();
            Val::new(s, v.truth)
        }
        "num" => {
            let v = arg(0);
            let width = arg(1).num().clamp(0, 64) as usize;
            let num = v.num();
            let s = if num < 0 { format!("-{:0width$}", -num) } else { format!("{num:0width$}") };
            Val::new(s, v.truth)
        }
        "add" | "sub" | "mul" | "div" | "mod" | "max" | "min" => {
            let vals = all();
            let truth = vals.iter().any(|v| v.truth);
            let mut it = vals.iter().map(Val::num);
            let first = it.next().unwrap_or(0);
            let res = it.fold(first, |acc, x| match name {
                "add" => acc.saturating_add(x),
                "sub" => acc.saturating_sub(x),
                "mul" => acc.saturating_mul(x),
                "div" => acc.checked_div(x).unwrap_or(0),
                "mod" => acc.checked_rem(x).unwrap_or(0),
                "max" => acc.max(x),
                _ => acc.min(x),
            });
            Val::new(res.to_string(), truth)
        }
        "muldiv" if n >= 3 => {
            let (a, b, c) = (arg(0).num(), arg(1).num(), arg(2).num());
            let res = if c == 0 { 0 } else { (a as i128 * b as i128 / c as i128) as i64 };
            Val::new(res.to_string(), true)
        }
        "filepart" => {
            let v = arg(0);
            let file = path_file(&v.text);
            let stem = match file.rfind('.') {
                Some(dot) if dot > 0 => &file[..dot],
                _ => file,
            };
            Val::new(stem, v.truth)
        }
        "filename" => {
            let v = arg(0);
            Val::new(path_file(&v.text), v.truth)
        }
        "fileext" | "ext" => {
            let v = arg(0);
            let file = path_file(&v.text);
            let ext = file.rfind('.').filter(|&d| d > 0).map(|d| &file[d + 1..]).unwrap_or("");
            Val::new(ext, v.truth)
        }
        "directory" => {
            let v = arg(0);
            let up = if n >= 2 { arg(1).num().max(1) as usize } else { 1 };
            let parts: Vec<&str> = v.text.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
            let s = parts.len().checked_sub(up + 1).map(|i| parts[i]).unwrap_or("");
            Val::new(s, v.truth)
        }
        "pathsafe" => {
            let v = arg(0);
            let s: String =
                v.text.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
            Val::new(s, v.truth)
        }
        "strchr" | "strrchr" => {
            let v = arg(0);
            let needle = arg(1).text.chars().next();
            let pos = needle.and_then(|c| {
                let chars: Vec<char> = v.text.chars().collect();
                if name == "strchr" {
                    chars.iter().position(|&x| x == c)
                } else {
                    chars.iter().rposition(|&x| x == c)
                }
            });
            Val::new(pos.map(|p| (p + 1).to_string()).unwrap_or_else(|| "0".into()), pos.is_some())
        }
        "strstr" => {
            let (h, nd) = (arg(0).text, arg(1).text);
            let pos = if nd.is_empty() { None } else { h.find(&nd).map(|b| h[..b].chars().count() + 1) };
            Val::new(pos.map(|p| p.to_string()).unwrap_or_else(|| "0".into()), pos.is_some())
        }
        "longest" | "shortest" => {
            let vals = all();
            let pick = if name == "longest" {
                vals.into_iter().max_by_key(|v| v.text.chars().count())
            } else {
                vals.into_iter().min_by_key(|v| v.text.chars().count())
            };
            pick.unwrap_or_default()
        }
        "char" => Val::new(char::from_u32(arg(0).num().clamp(0, 0x10FFFF) as u32).map(String::from).unwrap_or_default(), false),
        "crlf" => Val::new("\r\n", false),
        "tab" => Val::new("\t", false),
        _ => {
            // Unknown or wrongly-called function: show it like Winamp does.
            Val::new(format!("[UNKNOWN FUNCTION: {name}]"), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name: &str| map.get(name).cloned()
    }

    const DEFAULT: &str = crate::config::DEFAULT_TITLE_FORMAT;

    #[test]
    fn default_format() {
        let full = fields(&[("artist", "Daft Punk"), ("title", "One More Time"), ("filename", "/m/x.mp3")]);
        assert_eq!(format(DEFAULT, &full), "Daft Punk - One More Time");
        let no_artist = fields(&[("title", "Song"), ("filename", "/m/x.mp3")]);
        assert_eq!(format(DEFAULT, &no_artist), "Song");
        let none = fields(&[("filename", "/music/01 - Intro.flac")]);
        assert_eq!(format(DEFAULT, &none), "01 - Intro");
    }

    #[test]
    fn album_format() {
        let src = "[%artist% - ][%album% - ][$num(%track%,2) - ]$if2(%title%,$filepart(%filename%))";
        let f = fields(&[("artist", "A"), ("track", "3"), ("title", "T")]);
        assert_eq!(format(src, &f), "A - 03 - T");
    }

    #[test]
    fn nested_conditionals_and_escapes() {
        let f = fields(&[("artist", "A")]);
        assert_eq!(format("[x[%album%]y]", &f), "");
        assert_eq!(format("[x[%artist%]y]", &f), "xAy");
        assert_eq!(format("100%% $$5 [[ok]] 'it''s %x%'", &f), "100% $5 [ok] it's %x%");
    }

    #[test]
    fn functions() {
        let f = fields(&[("artist", "the beatles"), ("title", "Hey Jude"), ("len", "")]);
        assert_eq!(format("$caps(%artist%)", &f), "The Beatles");
        assert_eq!(format("$upper(%title%)|$lower(%title%)", &f), "HEY JUDE|hey jude");
        assert_eq!(format("$left(%title%,3)$right(%title%,2)", &f), "Heyde");
        assert_eq!(format("$padcut(%title%,5)|$pad(ab,4,.)|$lpad(7,3,0)", &f), "Hey J|ab..|007");
        assert_eq!(format("$if(%album%,yes,no)", &f), "no");
        assert_eq!(format("$if3(%album%,%x%,%title%,z)", &f), "Hey Jude");
        assert_eq!(format("$ifgreater(10,9,a,b)$iflonger(abc,2,c,d)", &f), "ac");
        assert_eq!(format("$add(1,2,3)$sub(10,4)$div(7,0)$muldiv(10,3,4)", &f), "6607");
        assert_eq!(format("$abbr(Hey Jude)$repeat(-,3)", &f), "HJ---");
        assert_eq!(format("$replace(%title%,e,3)", &f), "H3y Jud3");
        assert_eq!(format("$directory(/a/b/c.mp3)|$fileext(/a/b/c.mp3)|$filename(/a/b/c.mp3)", &f), "b|mp3|c.mp3");
        assert_eq!(format("$strchr(abcabc,c)$strrchr(abcabc,c)$strstr(abcabc,ca)", &f), "363");
        assert_eq!(format("$select(2,a,b,c)", &f), "b");
    }

    #[test]
    fn malformed_input_never_panics() {
        let f = fields(&[("title", "x")]);
        for src in ["$if(", "[%title%", "%title", "$", "$(", "]]]", "$foo(a)", "''''", "$num(abc,-5)", "[[[", "$left(%title%,-3)"] {
            let _ = format(src, &f);
        }
        assert_eq!(format("%title", &f), "%title");
        assert_eq!(format("$nosuch(a)", &f), "[UNKNOWN FUNCTION: nosuch]");
    }
}
