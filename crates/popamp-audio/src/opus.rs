//! Opus decoding through libopus, loaded at runtime like ALSA, so building
//! needs no Opus headers. Symphonia reads the container (Ogg, Matroska,
//! WebM) and trims the pre-skip; this only turns packets into samples.

use std::ffi::{c_int, c_uchar, c_void};
use std::sync::OnceLock;
use symphonia::core::audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec};
use symphonia::core::codecs::{CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult, CODEC_TYPE_OPUS};
use symphonia::core::errors::{decode_error, unsupported_error, Error, Result};
use symphonia::core::formats::Packet;
use symphonia::core::support_codec;

type State = *mut c_void;

struct Lib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn(i32, c_int, c_int, c_int, *const c_uchar, *mut c_int) -> State,
    decode: unsafe extern "C" fn(State, *const c_uchar, i32, *mut f32, c_int, c_int) -> c_int,
    destroy: unsafe extern "C" fn(State),
}

#[cfg(windows)]
const LIB_NAME: &str = "opus.dll";
#[cfg(target_os = "macos")]
const LIB_NAME: &str = "libopus.0.dylib";
#[cfg(linux_like)]
const LIB_NAME: &str = "libopus.so.0";

fn lib() -> std::result::Result<&'static Lib, String> {
    static LIB: OnceLock<std::result::Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib = crate::dylib::load(LIB_NAME)?;
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("libopus: missing {}: {e}", $name))?
            };
        }
        Ok(Lib {
            create: sym!("opus_multistream_decoder_create"),
            decode: sym!("opus_multistream_decode_float"),
            destroy: sym!("opus_multistream_decoder_destroy"),
            _lib: lib,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Opus always decodes at 48 kHz; the longest packet is 120 ms.
const RATE: u32 = 48_000;
const MAX_FRAMES: usize = 5_760;

/// What the `OpusHead` packet says about the streams inside.
#[derive(Debug, Clone, PartialEq)]
struct Head {
    channels: usize,
    streams: u8,
    coupled: u8,
    /// For each output channel, the decoded channel it comes from.
    mapping: Vec<u8>,
    /// Output gain to apply, as a factor.
    gain: f32,
}

fn parse_head(buf: &[u8]) -> Option<Head> {
    if buf.len() < 19 || &buf[..8] != b"OpusHead" {
        return None;
    }
    let channels = buf[9] as usize;
    let gain_db = i16::from_le_bytes([buf[16], buf[17]]) as f32 / 256.0;
    let gain = 10f32.powf(gain_db / 20.0);
    match buf[18] {
        0 if (1..=2).contains(&channels) => {
            Some(Head { channels, streams: 1, coupled: channels as u8 - 1, mapping: (0..channels as u8).collect(), gain })
        }
        1 if (1..=8).contains(&channels) && buf.len() >= 21 + channels => {
            Some(Head { channels, streams: buf[19], coupled: buf[20], mapping: buf[21..21 + channels].to_vec(), gain })
        }
        _ => None,
    }
}

/// The speakers in Vorbis order, the order Opus decodes them in.
fn vorbis_order(channels: usize) -> &'static [Channels] {
    use Channels as C;
    const L: C = C::FRONT_LEFT;
    const R: C = C::FRONT_RIGHT;
    const FC: C = C::FRONT_CENTRE;
    match channels {
        1 => &[L],
        2 => &[L, R],
        3 => &[L, FC, R],
        4 => &[L, R, C::REAR_LEFT, C::REAR_RIGHT],
        5 => &[L, FC, R, C::REAR_LEFT, C::REAR_RIGHT],
        6 => &[L, FC, R, C::REAR_LEFT, C::REAR_RIGHT, C::LFE1],
        7 => &[L, FC, R, C::SIDE_LEFT, C::SIDE_RIGHT, C::REAR_CENTRE, C::LFE1],
        _ => &[L, FC, R, C::SIDE_LEFT, C::SIDE_RIGHT, C::REAR_LEFT, C::REAR_RIGHT, C::LFE1],
    }
}

/// The speaker set, and for each decoded channel the plane it goes to:
/// Symphonia keeps planes in the order of the speaker bits.
fn layout(channels: usize) -> (Channels, Vec<usize>) {
    let order = vorbis_order(channels);
    let all = order.iter().fold(Channels::empty(), |a, &c| a | c);
    let planes = order.iter().map(|c| (all.bits() & (c.bits() - 1)).count_ones() as usize).collect();
    (all, planes)
}

pub struct OpusDecoder {
    params: CodecParameters,
    head: Head,
    state: State,
    planes: Vec<usize>,
    pcm: Vec<f32>,
    buf: AudioBuffer<f32>,
}

// The decoder state belongs to this value alone, and only `&mut self` touches it.
unsafe impl Send for OpusDecoder {}
unsafe impl Sync for OpusDecoder {}

impl OpusDecoder {
    fn new_state(head: &Head) -> Result<State> {
        let lib = lib().map_err(|e| {
            log::warn!("{e}");
            Error::Unsupported("Opus needs libopus")
        })?;
        let mut err = 0;
        let state = unsafe {
            (lib.create)(RATE as i32, head.channels as c_int, head.streams as c_int, head.coupled as c_int, head.mapping.as_ptr(), &mut err)
        };
        if state.is_null() || err != 0 {
            return decode_error("opus: bad stream layout");
        }
        Ok(state)
    }
}

impl Drop for OpusDecoder {
    fn drop(&mut self) {
        if let Ok(lib) = lib() {
            unsafe { (lib.destroy)(self.state) };
        }
    }
}

impl Decoder for OpusDecoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self> {
        let Some(head) = params.extra_data.as_deref().and_then(parse_head) else {
            return unsupported_error("opus: no OpusHead");
        };
        let state = Self::new_state(&head)?;
        let (channels, planes) = layout(head.channels);
        let buf = AudioBuffer::new(MAX_FRAMES as u64, SignalSpec::new(RATE, channels));
        Ok(OpusDecoder { params: params.clone(), pcm: vec![0.0; MAX_FRAMES * head.channels], head, state, planes, buf })
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_OPUS, "opus", "Opus")]
    }

    fn reset(&mut self) {
        // libopus resets through a variadic call; a new state does the same.
        if let Ok(state) = Self::new_state(&self.head) {
            if let Ok(lib) = lib() {
                unsafe { (lib.destroy)(self.state) };
            }
            self.state = state;
        }
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        self.buf.clear();
        let lib = lib().map_err(|_| Error::Unsupported("Opus needs libopus"))?;
        let ch = self.head.channels;
        let n = unsafe {
            (lib.decode)(self.state, packet.data.as_ptr(), packet.data.len() as i32, self.pcm.as_mut_ptr(), MAX_FRAMES as c_int, 0)
        };
        if n < 0 {
            return decode_error("opus: corrupt packet");
        }
        let frames = n as usize;
        self.buf.render_reserved(Some(frames));
        let gain = self.head.gain;
        for (i, &plane) in self.planes.iter().enumerate() {
            for (d, frame) in self.buf.chan_mut(plane).iter_mut().zip(self.pcm.chunks_exact(ch)) {
                *d = frame[i] * gain;
            }
        }
        self.buf.trim(packet.trim_start() as usize, packet.trim_end() as usize);
        Ok(self.buf.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.buf.as_audio_buffer_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surround_channels_land_on_their_speakers() {
        // Vorbis order L C R RL RR LFE; Symphonia's L R C LFE RL RR.
        assert_eq!(layout(6).1, [0, 2, 1, 4, 5, 3]);
        assert_eq!(layout(2).1, [0, 1]);
    }

    #[test]
    fn a_head_gives_the_streams_and_the_gain() {
        let mut head = b"OpusHead\x01\x02\x38\x01\x80\xbb\x00\x00\x00\x06\x00".to_vec();
        assert_eq!(
            parse_head(&head),
            Some(Head { channels: 2, streams: 1, coupled: 1, mapping: vec![0, 1], gain: 10f32.powf(6.0 / 20.0) })
        );
        head[18] = 2;
        assert_eq!(parse_head(&head), None, "unknown mapping family");
    }
}
