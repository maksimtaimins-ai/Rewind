//! AAC-LC encoding with the Media Foundation AAC encoder MFT (ships with
//! Windows). 48 kHz stereo 16-bit PCM in, raw AAC frames out.

use std::mem::ManuallyDrop;

use anyhow::{bail, Result};
use windows::core::GUID;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

use super::wasapi::{CHANNELS, SAMPLE_RATE};

const CLSID_AAC_ENCODER: GUID = GUID::from_u128(0x93af0c51_2275_45d2_a35b_f2ba21caed00);

pub struct AacFrame {
    pub pts: i64,
    pub duration: i64,
    pub data: Vec<u8>,
}

pub struct AacEncoder {
    t: IMFTransform,
    out_size: u32,
}
unsafe impl Send for AacEncoder {}

/// The MF encoder only accepts these rates (bits/s).
pub fn supported_bitrate(want: u32) -> u32 {
    *[96_000, 128_000, 160_000, 192_000].iter().min_by_key(|&&b: &&u32| b.abs_diff(want)).unwrap()
}

impl AacEncoder {
    pub fn new(bitrate: u32) -> Result<Self> {
        unsafe {
            let t: IMFTransform = CoCreateInstance(&CLSID_AAC_ENCODER, None, CLSCTX_INPROC_SERVER)?;
            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)?;
            out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
            out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, supported_bitrate(bitrate) / 8)?;
            out.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?; // raw frames
            out.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
            t.SetOutputType(0, &out, 0)?;

            let inp = MFCreateMediaType()?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)?;
            inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
            inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, CHANNELS as u32 * 2)?;
            inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, SAMPLE_RATE * CHANNELS as u32 * 2)?;
            t.SetInputType(0, &inp, 0)?;

            let info = t.GetOutputStreamInfo(0)?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(Self { t, out_size: info.cbSize.max(8192) })
        }
    }

    /// Feed interleaved 16-bit PCM starting at `pts` (100 ns).
    pub fn encode(&mut self, pcm: &[i16], pts: i64, out: &mut Vec<AacFrame>) -> Result<()> {
        if pcm.is_empty() {
            return Ok(());
        }
        unsafe {
            let bytes = pcm.len() * 2;
            let buf = MFCreateMemoryBuffer(bytes as u32)?;
            let mut p = std::ptr::null_mut();
            buf.Lock(&mut p, None, None)?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr() as *const u8, p, bytes);
            buf.Unlock()?;
            buf.SetCurrentLength(bytes as u32)?;
            let s = MFCreateSample()?;
            s.AddBuffer(&buf)?;
            let frames = (pcm.len() / CHANNELS as usize) as i64;
            s.SetSampleTime(pts)?;
            s.SetSampleDuration(frames * 10_000_000 / SAMPLE_RATE as i64)?;
            self.t.ProcessInput(0, &s, 0)?;
        }
        self.drain(out)
    }

    fn drain(&mut self, out: &mut Vec<AacFrame>) -> Result<()> {
        loop {
            unsafe {
                let s = MFCreateSample()?;
                s.AddBuffer(&MFCreateMemoryBuffer(self.out_size)?)?;
                let mut bufs = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: ManuallyDrop::new(Some(s)),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0;
                let r = self.t.ProcessOutput(0, &mut bufs, &mut status);
                let [b] = bufs;
                let sample = ManuallyDrop::into_inner(b.pSample);
                drop(ManuallyDrop::into_inner(b.pEvents));
                match r {
                    Ok(()) => {
                        let Some(sample) = sample else { continue };
                        let buf = sample.ConvertToContiguousBuffer()?;
                        let mut p = std::ptr::null_mut();
                        let mut len = 0;
                        buf.Lock(&mut p, None, Some(&mut len))?;
                        let data = std::slice::from_raw_parts(p, len as usize).to_vec();
                        buf.Unlock()?;
                        if !data.is_empty() {
                            out.push(AacFrame {
                                pts: sample.GetSampleTime().unwrap_or(0),
                                duration: sample.GetSampleDuration().unwrap_or(1024 * 10_000_000 / SAMPLE_RATE as i64),
                                data,
                            });
                        }
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                    Err(e) => bail!("AAC ProcessOutput: {e}"),
                }
            }
        }
    }
}
