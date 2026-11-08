//! The commands a plug-in may run, written out in its manifest. The user
//! reads them before allowing the plug-in; the plug-in only names one and
//! gives the values of its variables, which are checked against their
//! types, so what runs is what the user read.

use crate::manifest::{check_host_pattern, host_matches};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Values of variables, by name.
pub type Vars = BTreeMap<String, String>;

/// The user's proxy, as a URL; there only if the user has one.
pub const PROXY: &str = "proxy";
/// The start position; there only in a pipe that can seek.
pub const START: &str = "start";
/// Filled in by the player: never declared by a plug-in or given by one.
const BUILT_IN: [&str; 2] = [PROXY, START];

/// Programs that would run whatever they are told, so a fixed command
/// line says nothing about what they do.
const REFUSED: [&str; 39] = [
    "sh",
    "bash",
    "dash",
    "zsh",
    "ksh",
    "csh",
    "tcsh",
    "fish",
    "ash",
    "busybox",
    "cmd",
    "powershell",
    "pwsh",
    "python",
    "perl",
    "ruby",
    "node",
    "nodejs",
    "deno",
    "bun",
    "php",
    "lua",
    "awk",
    "gawk",
    "mawk",
    "wscript",
    "cscript",
    "mshta",
    "rundll32",
    "regsvr32",
    "osascript",
    "env",
    "xargs",
    "find",
    "nohup",
    "setsid",
    "sudo",
    "doas",
    "pkexec",
];

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    /// A program name, not a path; the setting of the same key, if the
    /// user changed it, says where the program is.
    pub program: String,
    pub args: Vec<Arg>,
    #[serde(default)]
    pub vars: BTreeMap<String, VarKind>,
}

/// One argument, or several that are left out together when a variable
/// among them has no value. Each is a fixed text or a whole `{variable}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Arg {
    One(String),
    Group(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum VarKind {
    /// An https address on one of these hosts.
    Url { hosts: Vec<String> },
    /// One line that is neither an option, a path nor an address.
    Text,
    /// Digits, and at most one point.
    Number,
    /// HTTP header lines, each ended by CR LF.
    Headers,
}

fn variable(arg: &str) -> Option<&str> {
    arg.strip_prefix('{')?.strip_suffix('}')
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The program's name without what only differs by system or version:
/// `Python3.12.EXE` is `python`.
fn bare_program(program: &str) -> String {
    let lower = program.to_ascii_lowercase();
    let stem = [".exe", ".com", ".cmd", ".bat", ".ps1"].iter().find_map(|e| lower.strip_suffix(e)).unwrap_or(&lower);
    stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.').to_string()
}

impl Command {
    /// Every argument, groups opened.
    fn all_args(&self) -> impl Iterator<Item = &String> {
        self.args.iter().flat_map(|a| match a {
            Arg::One(one) => std::slice::from_ref(one),
            Arg::Group(group) => group.as_slice(),
        })
    }

    /// Whether the manifest's text of this command makes sense.
    pub fn check(&self) -> Result<(), String> {
        let at = format!("command {:?}", self.id);
        if !is_name(&self.id) {
            return Err(format!("{at}: an id must be [a-z0-9_-]"));
        }
        if self.program.is_empty() || self.program.contains(['/', '\\']) {
            return Err(format!("{at}: program {:?} must be a program name, not a path", self.program));
        }
        if REFUSED.contains(&bare_program(&self.program).as_str()) {
            return Err(format!("{at}: {} would run anything it is told, so it may not be used", self.program));
        }
        self.check_args(&at)?;
        self.check_vars(&at)
    }

    fn check_args(&self, at: &str) -> Result<(), String> {
        if self.args.iter().any(|a| matches!(a, Arg::Group(g) if g.is_empty())) {
            return Err(format!("{at}: an empty group of arguments"));
        }
        for arg in self.all_args() {
            match variable(arg) {
                None if arg.contains(['{', '}']) => return Err(format!("{at}: {arg:?} must be a fixed text or a whole {{variable}}")),
                Some(name) if !self.vars.contains_key(name) && !BUILT_IN.contains(&name) => {
                    return Err(format!("{at}: variable {name:?} is not declared"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_vars(&self, at: &str) -> Result<(), String> {
        for (name, kind) in &self.vars {
            if BUILT_IN.contains(&name.as_str()) || !is_name(name) {
                return Err(format!("{at}: {name:?} cannot be the name of a variable"));
            }
            if !self.all_args().any(|a| variable(a) == Some(name)) {
                return Err(format!("{at}: variable {name:?} is not used"));
            }
            if let VarKind::Url { hosts } = kind {
                if hosts.is_empty() {
                    return Err(format!("{at}: variable {name:?} has no hosts"));
                }
                hosts.iter().try_for_each(|h| check_host_pattern(h))?;
            }
        }
        Ok(())
    }

    /// The arguments for these values. `given` comes from the plug-in;
    /// `built_in` from the player, and a name missing from it counts as
    /// having no value.
    pub fn expand(&self, given: &Vars, built_in: &Vars) -> Result<Vec<String>, String> {
        self.check_given(given)?;
        let fill = |arg: &String| match variable(arg) {
            Some(name) => given.get(name).or_else(|| built_in.get(name)).cloned(),
            None => Some(arg.clone()),
        };
        let mut args = Vec::new();
        for arg in &self.args {
            match arg {
                Arg::One(one) => args.push(fill(one).ok_or_else(|| format!("{one} has no value in command {:?}", self.id))?),
                Arg::Group(group) => args.extend(group.iter().map(fill).collect::<Option<Vec<_>>>().unwrap_or_default()),
            }
        }
        Ok(args)
    }

    fn check_given(&self, given: &Vars) -> Result<(), String> {
        given.iter().try_for_each(|(name, value)| match self.vars.get(name) {
            Some(kind) => kind.check(value).map_err(|why| format!("{name}: {why}")),
            None => Err(format!("{name:?} is not a variable of command {:?}", self.id)),
        })
    }

    /// The command as the user reads it; `[ ]` marks a part that may be
    /// left out.
    pub fn text(&self) -> String {
        let mut words = vec![self.program.clone()];
        words.extend(self.args.iter().map(|a| match a {
            Arg::One(one) => one.clone(),
            Arg::Group(group) => format!("[{}]", group.join(" ")),
        }));
        words.join(" ")
    }

    /// Where its addresses may lead, one text per address variable.
    pub fn limits(&self) -> Vec<String> {
        let hosts = |(name, kind): (&String, &VarKind)| match kind {
            VarKind::Url { hosts } => Some(format!("{{{name}}} only on {}", hosts.join(", "))),
            _ => None,
        };
        self.vars.iter().filter_map(hosts).collect()
    }

    /// Everything the user allows by allowing this command.
    pub fn canon(&self) -> String {
        let vars: Vec<String> = self.vars.iter().map(|(name, kind)| format!("{name}={kind:?}")).collect();
        format!("{}\n{}\n{}", self.id, self.text(), vars.join("\n"))
    }
}

impl VarKind {
    fn check(&self, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("is empty".into());
        }
        if value.starts_with('-') {
            return Err("may not start with -".into());
        }
        match self {
            VarKind::Url { hosts } => check_url(value, hosts),
            VarKind::Text => check_text(value),
            VarKind::Number => check_number(value),
            VarKind::Headers => check_headers(value),
        }
    }
}

/// Strict on purpose: the player and the program must not read a host
/// differently, so a user, a port or a backslash is not allowed at all.
fn check_url(value: &str, hosts: &[String]) -> Result<(), String> {
    let rest = value.strip_prefix("https://").ok_or("must start with https://")?;
    if value.chars().any(|c| c.is_control() || c.is_whitespace() || c == '\\') {
        return Err("has a character an address may not have".into());
    }
    let host = plain_host(rest).ok_or("must name a plain host")?;
    match host_matches(hosts, &host) {
        true => Ok(()),
        false => Err(format!("host {host} is not allowed")),
    }
}

/// The lowercase host `rest` starts with, if that is all there is before
/// the path.
fn plain_host(rest: &str) -> Option<String> {
    let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    (!host.is_empty() && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')).then_some(host)
}

/// A value like this could only be there to make the program open
/// something else than the command says.
fn check_text(value: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err("has a control character".into());
    }
    let drive = matches!(value.as_bytes(), [letter, b':', b'/' | b'\\', ..] if letter.is_ascii_alphabetic());
    if value.starts_with(['/', '\\', '~', '.']) || drive || value.contains("://") {
        return Err("may not be a path or an address".into());
    }
    Ok(())
}

fn check_number(value: &str) -> Result<(), String> {
    let fine = value.bytes().all(|b| b.is_ascii_digit() || b == b'.') && value.matches('.').count() <= 1 && value != ".";
    if fine {
        Ok(())
    } else {
        Err("must be a number".into())
    }
}

fn check_headers(value: &str) -> Result<(), String> {
    let lines = value.strip_suffix("\r\n").ok_or("must end with CR LF")?;
    for line in lines.split("\r\n") {
        let (name, text) = line.split_once(':').ok_or("must be lines of name: value")?;
        let name_fine = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if !name_fine || text.chars().any(char::is_control) {
            return Err("must be lines of name: value".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(toml_text: &str) -> Command {
        toml::from_str(toml_text).unwrap()
    }

    fn play() -> Command {
        command(
            r#"
id = "play"
program = "ffmpeg"
args = ["-nostdin", ["-user_agent", "{ua}"], ["-headers", "{headers}"], ["-http_proxy", "{proxy}"], ["-ss", "{start}"], "-t", "{limit}", "-i", "{url}", "pipe:1"]
[vars]
url = { type = "url", hosts = ["*.googlevideo.com", "youtu.be"] }
ua = { type = "text" }
headers = { type = "headers" }
limit = { type = "number" }
"#,
        )
    }

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const URL: &str = "https://rr1.googlevideo.com/videoplayback?id=1";

    #[test]
    fn values_land_in_their_places_and_a_part_without_a_value_is_left_out() {
        let args = play().expand(&vars(&[("url", URL), ("limit", "12.5")]), &Vars::new()).unwrap();
        assert_eq!(args, ["-nostdin", "-t", "12.5", "-i", URL, "pipe:1"]);
        let full = vars(&[("url", URL), ("limit", "1"), ("ua", "Mozilla/5.0 (X11)"), ("headers", "Referer: https://a/\r\nX-A: b\r\n")]);
        let built_in = vars(&[("proxy", "http://p:1"), ("start", "{start}")]);
        let args = play().expand(&full, &built_in).unwrap();
        assert_eq!(
            args,
            [
                "-nostdin",
                "-user_agent",
                "Mozilla/5.0 (X11)",
                "-headers",
                "Referer: https://a/\r\nX-A: b\r\n",
                "-http_proxy",
                "http://p:1",
                "-ss",
                "{start}",
                "-t",
                "1",
                "-i",
                URL,
                "pipe:1"
            ]
        );
    }

    #[test]
    fn a_plugin_cannot_add_to_a_command() {
        let good = vars(&[("url", URL), ("limit", "1")]);
        let with = |k: &str, v: &str| {
            let mut all = good.clone();
            all.insert(k.into(), v.into());
            play().expand(&all, &Vars::new())
        };
        assert_eq!(with("extra", "x").unwrap_err(), "\"extra\" is not a variable of command \"play\"");
        assert_eq!(with("proxy", "http://evil").unwrap_err(), "\"proxy\" is not a variable of command \"play\"");
        assert_eq!(with("start", "1").unwrap_err(), "\"start\" is not a variable of command \"play\"");
        assert_eq!(with("ua", "--exec=rm").unwrap_err(), "ua: may not start with -");
        assert_eq!(play().expand(&vars(&[("url", URL)]), &Vars::new()).unwrap_err(), "{limit} has no value in command \"play\"");
    }

    #[test]
    fn an_address_stays_on_the_hosts_the_user_read() {
        let url = |u: &str| play().expand(&vars(&[("url", u), ("limit", "1")]), &Vars::new()).map(|_| ());
        for good in [URL, "https://youtu.be/a", "https://YOUTU.BE/a?b=c@d", "https://a.googlevideo.com"] {
            assert_eq!(url(good), Ok(()), "{good}");
        }
        for (bad, why) in [
            ("file:///home/me/.ssh/id_rsa", "url: must start with https://"),
            ("http://youtu.be/a", "url: must start with https://"),
            ("concat:https://youtu.be/a", "url: must start with https://"),
            ("--exec=sh", "url: may not start with -"),
            ("", "url: is empty"),
            ("https://evil.org/", "url: host evil.org is not allowed"),
            ("https://youtu.be.evil.org/", "url: host youtu.be.evil.org is not allowed"),
            ("https://googlevideo.com/", "url: host googlevideo.com is not allowed"),
            ("https://evil.org#.youtu.be/", "url: host evil.org is not allowed"),
            ("https://youtu.be@evil.org/", "url: must name a plain host"),
            ("https://youtu.be:80@evil.org/", "url: must name a plain host"),
            ("https://youtu.be:8080/", "url: must name a plain host"),
            ("https://evil.org\\@youtu.be/", "url: has a character an address may not have"),
            ("https://youtu.be/a b", "url: has a character an address may not have"),
            ("https://youtu.be/a\nb", "url: has a character an address may not have"),
        ] {
            assert_eq!(url(bad), Err(why.to_string()), "{bad}");
        }
    }

    #[test]
    fn other_values_must_be_what_their_type_says() {
        let value = |k: &str, v: &str| {
            let mut all = vars(&[("url", URL), ("limit", "1")]);
            all.insert(k.into(), v.into());
            play().expand(&all, &Vars::new()).map(|_| ())
        };
        for good in ["0", "12", "12.5", "12."] {
            assert_eq!(value("limit", good), Ok(()), "{good}");
        }
        for bad in ["", ".", "1.2.3", "1e3", "0x10", "1 "] {
            assert!(value("limit", bad).is_err(), "{bad:?}");
        }
        for good in ["bestaudio[ext=m4a]/bestaudio", "Mozilla/5.0 (X11; Linux) Gecko", "a:b"] {
            assert_eq!(value("ua", good), Ok(()), "{good}");
        }
        for bad in ["/etc/passwd", "~/.ssh/id_rsa", "./x", "../x", "C:\\x", "c:/x", "\\\\host\\x", "https://evil.org", "a\nb", "a\tb"] {
            assert!(value("ua", bad).is_err(), "{bad:?}");
        }
        assert_eq!(value("headers", "A: b\r\n"), Ok(()));
        assert_eq!(value("headers", "Set-Cookie: a=b; c=d\r\nB:\r\n"), Ok(()));
        for bad in ["A: b", "A b\r\n", ": b\r\n", "A: b\nC: d\r\n", "A: b\r\n\r\n", "A B: c\r\n"] {
            assert!(value("headers", bad).is_err(), "{bad:?}");
        }
    }

    fn check(program: &str, args: &str, vars: &str) -> Result<(), String> {
        command(&format!("id = \"a\"\nprogram = \"{program}\"\nargs = {args}\n{vars}")).check()
    }

    #[test]
    fn a_command_must_say_exactly_what_it_runs() {
        assert_eq!(check("ffmpeg", "[\"-i\", \"{u}\"]", "[vars]\nu = { type = \"url\", hosts = [\"a.b\"] }"), Ok(()));
        assert_eq!(check("ffmpeg", "[\"-ss\", \"{start}\", \"{proxy}\"]", ""), Ok(()));
        for ((program, args, vars), why) in [
            (("/usr/bin/ffmpeg", "[]", ""), "must be a program name, not a path"),
            (("..\\\\ffmpeg", "[]", ""), "must be a program name, not a path"),
            (("", "[]", ""), "must be a program name, not a path"),
            (("ffmpeg", "[\"{u}\"]", ""), "variable \"u\" is not declared"),
            (("ffmpeg", "[\"x={u}\"]", "[vars]\nu = { type = \"text\" }"), "must be a fixed text or a whole {variable}"),
            (("ffmpeg", "[]", "[vars]\nu = { type = \"text\" }"), "variable \"u\" is not used"),
            (("ffmpeg", "[\"{proxy}\"]", "[vars]\nproxy = { type = \"text\" }"), "\"proxy\" cannot be the name of a variable"),
            (("ffmpeg", "[[]]", ""), "an empty group of arguments"),
            (("ffmpeg", "[\"{u}\"]", "[vars]\nu = { type = \"url\", hosts = [] }"), "variable \"u\" has no hosts"),
            (("ffmpeg", "[\"{u}\"]", "[vars]\nu = { type = \"url\", hosts = [\"*\"] }"), "must be a lowercase name"),
        ] {
            let err = check(program, args, vars).unwrap_err();
            assert!(err.contains(why), "{program} {args} {vars}: {err}");
        }
    }

    #[test]
    fn programs_that_run_anything_are_refused_however_they_are_written() {
        for program in
            ["sh", "bash", "SH.EXE", "cmd.exe", "powershell.exe", "python3", "python3.12", "Python.exe", "perl5.36", "env", "busybox"]
        {
            let err = check(program, "[]", "").unwrap_err();
            assert!(err.contains("would run anything"), "{program}: {err}");
        }
        for program in ["ffmpeg", "yt-dlp", "shntool", "mp3gain", "echo"] {
            assert_eq!(check(program, "[]", ""), Ok(()), "{program}");
        }
    }

    #[test]
    fn the_user_reads_the_command_and_any_change_counts() {
        let p = play();
        assert_eq!(
            p.text(),
            "ffmpeg -nostdin [-user_agent {ua}] [-headers {headers}] [-http_proxy {proxy}] [-ss {start}] -t {limit} -i {url} pipe:1"
        );
        assert_eq!(p.limits(), ["{url} only on *.googlevideo.com, youtu.be"]);
        let mut other = p.clone();
        other.vars.insert("url".into(), VarKind::Url { hosts: vec!["evil.org".into()] });
        assert_ne!(p.canon(), other.canon());
    }
}
