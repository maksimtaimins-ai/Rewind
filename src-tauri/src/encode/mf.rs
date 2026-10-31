//! Media Foundation H.264/HEVC encoder MFTs.
//!
//! Hardware MFTs (NVENC / AMF / QuickSync) are async: they post NeedInput /
//! HaveOutput events and take D3D11 textures directly. The Microsoft software
//! MFT is synchronous and takes system-memory NV12.

use std::mem::ManuallyDrop;

use anyhow::{anyhow, bail, Context, Result};
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::E_NOTIMPL;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4};

use super::{EncoderInfo, EncoderKind, EncoderParams};
use crate::gpu::Gpu;
use crate::settings::{Codec, RateControl};

pub fn mf_startup() {
    unsafe {
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    }
}

fn variant_u32(v: u32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_UI4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { ulVal: v },
            }),
        },
    }
}

fn variant_bool(v: bool) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BOOL,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { boolVal: windows::Win32::Foundation::VARIANT_BOOL(if v { -1 } else { 0 }) },
            }),
        },
    }
}

fn subtype(codec: Codec) -> GUID {
    match codec {
        Codec::H264 => MFVideoFormat_H264,
        Codec::Hevc => MFVideoFormat_HEVC,
    }
}

fn get_string(a: &IMFAttributes, key: &GUID) -> Option<String> {
    unsafe {
        let mut p = PWSTR::null();
        let mut len = 0;
        a.GetAllocatedString(key, &mut p, &mut len).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

/// Hardware encoder activates for `codec` on one adapter. Enumerating per
/// adapter (MFTEnum2) is what ties each MFT to its GPU on multi-GPU systems.
fn adapter_activates(codec: Codec, luid: u64) -> Vec<IMFActivate> {
    let mut out = Vec::new();
    unsafe {
        let mut attrs = None;
        if MFCreateAttributes(&mut attrs, 1).is_err() {
            return out;
        }
        let Some(attrs) = attrs else { return out };
        if attrs.SetBlob(&MFT_ENUM_ADAPTER_LUID, &luid.to_le_bytes()).is_err() {
            return out;
        }
        let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
        let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: subtype(codec) };
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
        if MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &attrs, &mut list, &mut count).is_ok()
            && !list.is_null()
        {
            for a in std::slice::from_raw_parts_mut(list, count as usize) {
                if let Some(act) = a.take() {
                    out.push(act);
                }
            }
            CoTaskMemFree(Some(list as _));
        }
    }
    out
}

/// Enumerate hardware encoder MFTs for `codec` on every GPU, plus the
/// software H.264 MFT.
pub fn enumerate(codec: Codec) -> Vec<EncoderInfo> {
    let mut out: Vec<EncoderInfo> = Vec::new();
    for adapter in crate::gpu::adapters().unwrap_or_default() {
        for (i, act) in adapter_activates(codec, adapter.luid).into_iter().enumerate() {
            let Ok(attrs) = act.cast::<IMFAttributes>() else { continue };
            let name = get_string(&attrs, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_else(|| format!("Encoder {i}"));
            let vendor = get_string(&attrs, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute).unwrap_or_default();
            let kind = match vendor.to_ascii_uppercase().as_str() {
                v if v.contains("10DE") => EncoderKind::Nvenc,
                v if v.contains("1002") || v.contains("1022") => EncoderKind::Amf,
                v if v.contains("8086") => EncoderKind::Qsv,
                _ => match adapter.vendor_id {
                    0x10de => EncoderKind::Nvenc,
                    0x1002 | 0x1022 => EncoderKind::Amf,
                    0x8086 => EncoderKind::Qsv,
                    _ => EncoderKind::Other,
                },
            };
            if out.iter().any(|e| e.name == name && e.luid == Some(adapter.luid)) {
                continue; // some drivers register the same MFT twice
            }
            out.push(EncoderInfo { kind, name, codec, luid: Some(adapter.luid), index: i as u32 });
        }
    }
    if codec == Codec::H264 {
        out.push(EncoderInfo {
            kind: EncoderKind::Software,
            name: "Microsoft H.264 (CPU)".into(),
            codec,
            luid: None,
            index: u32::MAX,
        });
    }
    out
}

fn activate(info: &EncoderInfo) -> Result<(IMFTransform, Option<IMFActivate>)> {
    unsafe {
        if info.kind == EncoderKind::Software {
            return Ok((CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER)?, None));
        }
        let luid = info.luid.ok_or_else(|| anyhow!("hardware encoder without adapter"))?;
        let act = adapter_activates(info.codec, luid)
            .into_iter()
            .nth(info.index as usize)
            .ok_or_else(|| anyhow!("encoder {} no longer present", info.name))?;
        let t = act.ActivateObject::<IMFTransform>().context("ActivateObject")?;
        Ok((t, Some(act)))
    }
}

pub struct EncodedFrame {
    pub pts: i64,
    pub duration: i64,
    pub keyframe: bool,
    /// Annex B bitstream.
    pub data: Vec<u8>,
}

pub struct MfEncoder {
    transform: IMFTransform,
    // Both must outlive the transform: the MFT keeps raw pointers to the
    // device manager, and hardware MFTs expect ShutdownObject on the activate.
    activate: Option<IMFActivate>,
    _manager: Option<IMFDXGIDeviceManager>,
    events: Option<IMFMediaEventGenerator>,
    codec_api: Option<ICodecAPI>,
    gpu: Gpu,
    params: EncoderParams,
    output_id: u32,
    input_id: u32,
    provides_samples: bool,
    output_size: u32,
    software: bool,
    staging: Option<ID3D11Texture2D>,
    /// Hardware MFTs ask for input explicitly.
    pending_input: u32,
    frame_ticks: i64,
    pub sequence_header: Option<Vec<u8>>,
    shut_down: bool,
}
unsafe impl Send for MfEncoder {}

impl MfEncoder {
    pub fn new(info: &EncoderInfo, gpu: &Gpu, params: EncoderParams) -> Result<Self> {
        let (transform, activate) = activate(info)?;
        let software = info.kind == EncoderKind::Software;
        unsafe {
            let attrs = transform.GetAttributes().ok();
            let is_async = attrs.as_ref().map(|a| a.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0).unwrap_or(false);
            if is_async {
                attrs.as_ref().unwrap().SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            }
            let events = if is_async { Some(transform.cast::<IMFMediaEventGenerator>()?) } else { None };

            let (mut ins, mut outs) = ([0u32], [0u32]);
            match transform.GetStreamIDs(&mut ins, &mut outs) {
                Ok(()) => {}
                // Fixed stream IDs: both are 0.
                Err(e) if e.code() == E_NOTIMPL => {
                    ins = [0];
                    outs = [0];
                }
                Err(e) => return Err(e.into()),
            }
            let (input_id, output_id) = (ins[0], outs[0]);

            let mut manager = None;
            if !software {
                let mut token = 0u32;
                MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
                let m = manager.as_ref().ok_or_else(|| anyhow!("no DXGI device manager"))?;
                m.ResetDevice(&gpu.device, token)?;
                transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, m.as_raw() as usize)?;
            }

            let codec_api = transform.cast::<ICodecAPI>().ok();
            let mut enc = Self {
                transform,
                activate,
                _manager: manager,
                events,
                codec_api,
                gpu: gpu.clone(),
                output_id,
                input_id,
                provides_samples: false,
                output_size: 0,
                software,
                staging: None,
                pending_input: 0,
                frame_ticks: 10_000_000 / params.fps.max(1) as i64,
                sequence_header: None,
                shut_down: false,
                params,
            };
            enc.apply_codec_api();
            enc.set_types()?;
            enc.apply_codec_api(); // some MFTs only accept these after types are set

            let info = enc.transform.GetOutputStreamInfo(enc.output_id)?;
            enc.provides_samples = info.dwFlags
                & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32)
                != 0;
            enc.output_size = info.cbSize.max(enc.params.width * enc.params.height * 2);

            // The software MFT rejects a flush before streaming starts; harmless.
            let _ = enc.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            enc.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            enc.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(enc)
        }
    }

    fn set(&self, api: &GUID, v: VARIANT) {
        if let Some(c) = &self.codec_api {
            unsafe {
                let _ = c.SetValue(api, &v);
            }
        }
    }

    fn apply_codec_api(&self) {
        let p = &self.params;
        let mode = match p.rate_control {
            RateControl::Cbr => 0,     // eAVEncCommonRateControlMode_CBR
            RateControl::Vbr => 1,     // PeakConstrainedVBR
            RateControl::Quality => 3, // Quality
        };
        self.set(&CODECAPI_AVEncCommonRateControlMode, variant_u32(mode));
        self.set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(p.bitrate_kbps * 1000));
        self.set(&CODECAPI_AVEncCommonMaxBitRate, variant_u32(p.bitrate_kbps * 1500));
        if p.rate_control == RateControl::Quality {
            self.set(&CODECAPI_AVEncCommonQuality, variant_u32(p.quality.clamp(1, 100)));
        }
        // Keyframes are forced on a timer (see encode::run); the GOP size is only
        // a backstop, set long so it doesn't add extra IDRs next to forced ones.
        let gop = ((p.fps as f32 * p.keyframe_secs * 4.0).round() as u32).max(1);
        self.set(&CODECAPI_AVEncMPVGOPSize, variant_u32(gop));
        // No B-frames: pts == dts, lower latency, simpler muxing and cutting.
        self.set(&CODECAPI_AVEncMPVDefaultBPictureCount, variant_u32(0));
        self.set(&CODECAPI_AVLowLatencyMode, variant_bool(true));
        self.set(&CODECAPI_AVEncCommonQualityVsSpeed, variant_u32(50));
    }

    unsafe fn set_types(&mut self) -> Result<()> {
        unsafe {
            let p = &self.params;
            let size = ((p.width as u64) << 32) | p.height as u64;
            let rate = ((p.fps as u64) << 32) | 1;

            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &subtype(p.codec))?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, p.bitrate_kbps * 1000)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            out.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            let profile = match p.codec {
                Codec::H264 => eAVEncH264VProfile_High.0 as u32,
                Codec::Hevc => eAVEncH265VProfile_Main_420_8.0 as u32,
            };
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, profile)?;
            out.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
            self.transform.SetOutputType(self.output_id, &out, 0).context("SetOutputType")?;

            // Pick the encoder's own NV12 input type so vendor-specific attributes survive.
            let mut chosen = None;
            for i in 0.. {
                let Ok(t) = self.transform.GetInputAvailableType(self.input_id, i) else { break };
                if t.GetGUID(&MF_MT_SUBTYPE).ok() == Some(MFVideoFormat_NV12) {
                    chosen = Some(t);
                    break;
                }
            }
            let input = match chosen {
                Some(t) => t,
                None => {
                    let t = MFCreateMediaType()?;
                    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                    t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
                    t
                }
            };
            input.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            input.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            self.transform.SetInputType(self.input_id, &input, 0).context("SetInputType")?;
            Ok(())
        }
    }

    pub fn is_async(&self) -> bool {
        self.events.is_some()
    }

    pub fn force_keyframe(&self) {
        self.set(&CODECAPI_AVEncVideoForceKeyFrame, variant_u32(1));
    }

    fn make_sample(&mut self, tex: &ID3D11Texture2D, pts: i64) -> Result<IMFSample> {
        unsafe {
            let sample = MFCreateSample()?;
            if self.software {
                // CPU path: read NV12 back and pack planes tightly.
                let (w, h) = (self.params.width, self.params.height);
                if self.staging.is_none() {
                    self.staging = Some(self.gpu.staging(w, h, DXGI_FORMAT_NV12)?);
                }
                let staging = self.staging.as_ref().unwrap();
                let ctx = &self.gpu.context;
                ctx.CopyResource(staging, tex);
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
                let len = (w * h * 3 / 2) as usize;
                let buf = MFCreateMemoryBuffer(len as u32)?;
                let mut dst = std::ptr::null_mut();
                buf.Lock(&mut dst, None, None)?;
                let pitch = map.RowPitch as usize;
                let src = map.pData as *const u8;
                for row in 0..(h as usize * 3 / 2) {
                    std::ptr::copy_nonoverlapping(src.add(row * pitch), dst.add(row * w as usize), w as usize);
                }
                buf.Unlock()?;
                buf.SetCurrentLength(len as u32)?;
                ctx.Unmap(staging, 0);
                sample.AddBuffer(&buf)?;
            } else {
                let buf = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, tex, 0, false)?;
                if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
                    if let Ok(len) = b2.GetContiguousLength() {
                        let _ = buf.SetCurrentLength(len);
                    }
                }
                sample.AddBuffer(&buf)?;
            }
            sample.SetSampleTime(pts)?;
            sample.SetSampleDuration(self.frame_ticks)?;
            Ok(sample)
        }
    }

    /// Wait for the next event of an async MFT. Returns false when the event
    /// was "need input" (caller should supply a frame).
    #[allow(non_upper_case_globals)]
    pub fn wait_event(&mut self, out: &mut Vec<EncodedFrame>) -> Result<bool> {
        let Some(ev) = &self.events else { return Ok(false) };
        let event = unsafe { ev.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))? };
        let t = unsafe { event.GetType()? };
        match MF_EVENT_TYPE(t as i32) {
            METransformNeedInput => {
                self.pending_input += 1;
                Ok(false)
            }
            METransformHaveOutput => {
                self.drain_output(out)?;
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    pub fn needs_input(&self) -> bool {
        self.events.is_none() || self.pending_input > 0
    }

    pub fn encode(&mut self, tex: &ID3D11Texture2D, pts: i64, out: &mut Vec<EncodedFrame>) -> Result<()> {
        let sample = self.make_sample(tex, pts)?;
        unsafe { self.transform.ProcessInput(self.input_id, &sample, 0).context("ProcessInput")? };
        if self.events.is_some() {
            self.pending_input = self.pending_input.saturating_sub(1);
        } else {
            self.drain_output(out)?;
        }
        Ok(())
    }

    fn drain_output(&mut self, out: &mut Vec<EncodedFrame>) -> Result<()> {
        loop {
            let alloc = if self.provides_samples {
                None
            } else {
                unsafe {
                    let s = MFCreateSample()?;
                    s.AddBuffer(&MFCreateMemoryBuffer(self.output_size)?)?;
                    Some(s)
                }
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: self.output_id,
                pSample: ManuallyDrop::new(alloc),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0u32;
            let r = unsafe { self.transform.ProcessOutput(0, &mut buffers, &mut status) };
            let [b] = buffers;
            let sample = ManuallyDrop::into_inner(b.pSample);
            drop(ManuallyDrop::into_inner(b.pEvents));
            match r {
                Ok(()) => {
                    if let Some(s) = sample {
                        out.push(self.read_sample(&s)?);
                    }
                    if self.events.is_some() {
                        return Ok(()); // async: one output per HaveOutput event
                    }
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => unsafe {
                    let t = self.transform.GetOutputAvailableType(self.output_id, 0)?;
                    self.transform.SetOutputType(self.output_id, &t, 0)?;
                },
                Err(e) => bail!("ProcessOutput: {e}"),
            }
        }
    }

    fn read_sample(&mut self, s: &IMFSample) -> Result<EncodedFrame> {
        unsafe {
            let buf = s.ConvertToContiguousBuffer()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut len))?;
            let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
            buf.Unlock()?;
            let keyframe = s.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
            if keyframe && self.sequence_header.is_none() {
                if let Ok(t) = self.transform.GetOutputCurrentType(self.output_id) {
                    let mut size = 0;
                    if t.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).map(|s| size = s).is_ok() && size > 0 {
                        let mut blob = vec![0u8; size as usize];
                        if t.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None).is_ok() {
                            self.sequence_header = Some(blob);
                        }
                    }
                }
            }
            Ok(EncodedFrame {
                pts: s.GetSampleTime().unwrap_or(0),
                duration: s.GetSampleDuration().unwrap_or(self.frame_ticks),
                keyframe,
                data,
            })
        }
    }

    /// Drain and shut down. Async MFTs keep work items queued on MF threads;
    /// releasing them before METransformDrainComplete crashes inside the driver.
    #[allow(non_upper_case_globals)]
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        self.shut_down = true;
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, self.input_id as usize);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, self.input_id as usize);
            if let Some(ev) = self.events.clone() {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                let mut discard = Vec::new();
                while std::time::Instant::now() < deadline {
                    match ev.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                        Ok(e) => match MF_EVENT_TYPE(e.GetType().unwrap_or(0) as i32) {
                            METransformDrainComplete => break,
                            METransformHaveOutput => {
                                let _ = self.drain_output(&mut discard);
                                discard.clear();
                            }
                            _ => {}
                        },
                        Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => {
                            std::thread::sleep(std::time::Duration::from_millis(2))
                        }
                        Err(_) => break,
                    }
                }
                if let Ok(sh) = ev.cast::<IMFShutdown>() {
                    let _ = sh.Shutdown();
                }
            } else {
                let mut discard = Vec::new();
                let _ = self.drain_output(&mut discard);
            }
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            if let Some(a) = self.activate.take() {
                let _ = a.ShutdownObject();
            }
        }
    }
}

impl Drop for MfEncoder {
    fn drop(&mut self) {
        self.shutdown();
        // NVIDIA's MFT can still have work items in flight on MF work queues
        // right after drain + shutdown; releasing it immediately races them
        // (access violation in nvEncMFT*.dll). Release a second later instead.
        let held = crate::gpu::SendPtr((self.transform.clone(), self.events.take(), self._manager.take()));
        let _ = std::thread::Builder::new().name("rewind-mft-release".into()).spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            drop(held);
        });
    }
}
