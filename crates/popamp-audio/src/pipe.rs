//! Plays what a program writes to its standard output: interleaved stereo
//! 32-bit float samples, little-endian, at a rate the source names.
//!
//! The program knows nothing of Popamp; a source decides which program to
//! run and with which arguments (YouTube runs ffmpeg on a stream URL).

use crate::decoder::{AudioSource, Status, StreamInfo};
use popamp_core::meta::TrackMeta;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

/// In an argument, replaced by the start position in seconds (`12.345`).
pub const START: &str = "{start}";

/// How to play a track through a program.
#[derive(Debug, Clone, PartialEq)]
pub struct Pipe {
    /// A path, or a name looked up in `PATH`.
    pub program: String,
    /// Arguments; [`START`] in any of them is replaced each time the program
    /// starts: when the track opens and on every seek.
    pub args: Vec<String>,
    pub rate: u32,
    pub seekable: bool,
    /// Tags of the track, known before playing (the samples carry none).
    pub meta: TrackMeta,
}

impl Pipe {
    fn args_at(&self, ms: u64) -> Vec<String> {
        let at = format!("{}.{:03}", ms / 1000, ms % 1000);
        self.args.iter().map(|a| a.replace(START, &at)).collect()
    }

    fn name(&self) -> String {
        Path::new(&self.program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.program.clone())
    }
}

pub struct PipeSource {
    pipe: Pipe,
    child: Option<Child>,
    stdout: Option<ChildStdout>,
    info: StreamInfo,
    bytes: Vec<u8>,
    /// Bytes carried over between reads (partial samples).
    carry: Vec<u8>,
    produced: bool,
}

impl PipeSource {
    pub fn open(pipe: Pipe, start_ms: u64) -> Result<(PipeSource, TrackMeta), String> {
        let meta = pipe.meta.clone();
        let info = StreamInfo {
            sample_rate: pipe.rate,
            channels: meta.channels.unwrap_or(2),
            length_ms: meta.length_ms,
            bitrate_kbps: meta.bitrate_kbps,
            codec: meta.codec.clone().unwrap_or_else(|| "PIPE".into()),
            seekable: pipe.seekable,
        };
        let mut src = PipeSource { pipe, child: None, stdout: None, info, bytes: vec![0; 16384], carry: Vec::new(), produced: false };
        src.spawn(start_ms)?;
        Ok((src, meta))
    }

    fn spawn(&mut self, start_ms: u64) -> Result<(), String> {
        self.kill();
        let mut cmd = Command::new(&self.pipe.program);
        cmd.args(self.pipe.args_at(start_ms)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{} not found", self.pipe.program)
            } else {
                format!("cannot run {}: {e}", self.pipe.program)
            }
        })?;
        self.stdout = child.stdout.take();
        self.child = Some(child);
        self.carry.clear();
        self.produced = false;
        Ok(())
    }

    fn kill(&mut self) {
        self.stdout = None;
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// The child's error output once it has exited.
    fn failure(&mut self) -> Option<String> {
        let mut child = self.child.take()?;
        let status = child.wait().ok()?;
        if status.success() {
            return None;
        }
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let line = err.lines().last().unwrap_or("").trim().to_string();
        let name = self.pipe.name();
        Some(if line.is_empty() { format!("{name} exited with {status}") } else { format!("{name}: {line}") })
    }
}

impl AudioSource for PipeSource {
    fn info(&self) -> &StreamInfo {
        &self.info
    }

    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        let Some(stdout) = self.stdout.as_mut() else { return Ok(Status::Eof) };
        let n = match stdout.read(&mut self.bytes) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => return Ok(Status::Data),
            Err(e) => return Err(format!("{} pipe: {e}", self.pipe.name())),
        };
        if n == 0 {
            self.stdout = None;
            if let Some(err) = self.failure() {
                if !self.produced {
                    return Err(err);
                }
                log::warn!("{err}");
            }
            return Ok(Status::Eof);
        }
        self.produced = true;
        self.carry.extend_from_slice(&self.bytes[..n]);
        let whole = self.carry.len() / 8 * 8;
        out.extend(self.carry[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
        self.carry.drain(..whole);
        Ok(Status::Data)
    }

    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        if !self.info.seekable {
            return Err("live stream".into());
        }
        let ms = self.info.length_ms.map(|l| ms.min(l.saturating_sub(250))).unwrap_or(ms);
        self.spawn(ms)?;
        Ok(ms)
    }
}

impl Drop for PipeSource {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A pipe that runs a shell script; `$1` is the start position.
    pub(crate) fn sh(script: &str) -> Pipe {
        Pipe {
            program: "sh".into(),
            args: vec!["-c".into(), script.into(), "sh".into(), START.into()],
            rate: 48_000,
            seekable: true,
            meta: TrackMeta { length_ms: Some(10_000), ..Default::default() },
        }
    }

    fn drain(src: &mut PipeSource) -> Result<Vec<f32>, String> {
        let mut out = Vec::new();
        while let Status::Data = src.next(&mut out)? {}
        Ok(out)
    }

    #[test]
    fn plays_the_samples_a_program_writes() {
        let (mut src, _) = PipeSource::open(sh("head -c 8000 /dev/zero"), 0).unwrap();
        let out = drain(&mut src).unwrap();
        assert_eq!(out.len(), 2000);
        assert!(out.iter().all(|s| *s == 0.0));
        assert_eq!(src.info().sample_rate, 48_000);
    }

    #[test]
    fn starts_and_seeks_the_program_at_the_position_asked() {
        let pipe = sh(r#"case "$1" in 1.500|4.250) head -c 8 /dev/zero;; *) echo "at $1" >&2; exit 1;; esac"#);
        let (mut src, _) = PipeSource::open(pipe, 1500).unwrap();
        assert_eq!(drain(&mut src).unwrap().len(), 2);
        assert_eq!(src.seek(4250).unwrap(), 4250);
        assert_eq!(drain(&mut src).unwrap().len(), 2);
        assert_eq!(src.seek(2000).unwrap(), 2000);
        assert_eq!(drain(&mut src).unwrap_err(), "sh: at 2.000");
    }

    #[test]
    fn a_live_pipe_does_not_seek() {
        let (mut src, _) = PipeSource::open(Pipe { seekable: false, ..sh("head -c 8 /dev/zero") }, 0).unwrap();
        assert!(src.seek(1000).is_err());
    }

    #[test]
    fn a_missing_program_is_named() {
        let pipe = Pipe { program: "popamp-no-such-program".into(), ..sh("") };
        assert_eq!(PipeSource::open(pipe, 0).err().unwrap(), "popamp-no-such-program not found");
    }
}
