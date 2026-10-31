//! WASAPI capture: endpoint loopback, per-process loopback (Windows 10 2004+)
//! and microphones. Every source is opened as 48 kHz stereo f32 (WASAPI
//! converts), so the rest of the audio path never resamples.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use windows::core::{implement, Interface, Ref, HSTRING};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, BLOB, CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;
use windows::Win32::Foundation::PROPERTYKEY;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;

pub enum Source {
    /// Default render endpoint loopback.
    System,
    /// Process-tree loopback; `exclude` captures everything *but* that tree.
    Process { pid: u32, exclude: bool },
    /// Capture endpoint; `None` = default communications/console mic.
    Mic(Option<String>),
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

fn enumerator() -> Result<IMMDeviceEnumerator> {
    Ok(unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? })
}

// PKEY_Device_FriendlyName
const PKEY_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

pub fn microphones() -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(e) = enumerator() else { return out };
        let Ok(coll) = e.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return out };
        let n = coll.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(dev) = coll.Item(i) else { continue };
            let Ok(id) = dev.GetId() else { continue };
            let id_s = id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(id.0 as _));
            let name = dev
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&PKEY_FRIENDLY_NAME))
                .map(|v| v.to_string())
                .unwrap_or_else(|_| id_s.clone());
            out.push(DeviceInfo { id: id_s, name });
        }
    }
    out
}

fn float_format() -> WAVEFORMATEX {
    let block = CHANNELS * 4;
    WAVEFORMATEX {
        wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * block as u32,
        nBlockAlign: block,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivateHandler(mpsc::SyncSender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivateHandler_Impl {
    fn ActivateCompleted(&self, _op: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

fn process_loopback_client(pid: u32, exclude: bool) -> Result<IAudioClient> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: if exclude {
                    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                } else {
                    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                },
            },
        },
    };
    let prop = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: &mut params as *mut _ as *mut u8,
                    },
                },
            }),
        },
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let handler: IActivateAudioInterfaceCompletionHandler = ActivateHandler(tx).into();
    unsafe {
        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&prop),
            &handler,
        )
        .context("ActivateAudioInterfaceAsync (needs Windows 10 2004+)")?;
        // The blob points at stack memory; don't let PROPVARIANT's Drop free it.
        std::mem::forget(prop);
        rx.recv_timeout(Duration::from_secs(5)).map_err(|_| anyhow!("process loopback activation timed out"))?;
        let mut hr = windows::core::HRESULT(0);
        let mut unk = None;
        op.GetActivateResult(&mut hr, &mut unk)?;
        hr.ok()?;
        Ok(unk.ok_or_else(|| anyhow!("no audio client"))?.cast()?)
    }
}

pub struct Capture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
}
unsafe impl Send for Capture {}

/// One chunk of captured audio: interleaved stereo f32 starting at `qpc`
/// (100 ns QPC units). `silent` chunks carry no samples, only a length.
pub struct Chunk<'a> {
    pub qpc: i64,
    pub frames: usize,
    pub samples: Option<&'a [f32]>,
}

impl Capture {
    pub fn open(source: &Source) -> Result<Self> {
        unsafe {
            let (client, loopback) = match source {
                Source::System => {
                    let dev = enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?;
                    (dev.Activate::<IAudioClient>(CLSCTX_ALL, None)?, true)
                }
                Source::Process { pid, exclude } => (process_loopback_client(*pid, *exclude)?, true),
                Source::Mic(id) => {
                    let e = enumerator()?;
                    let dev = match id {
                        Some(id) => e.GetDevice(&HSTRING::from(id.as_str()))?,
                        None => e.GetDefaultAudioEndpoint(eCapture, eConsole)?,
                    };
                    (dev.Activate::<IAudioClient>(CLSCTX_ALL, None)?, false)
                }
            };
            let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            if loopback {
                flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
            }
            let fmt = float_format();
            // 100 ms device buffer: plenty of slack if the thread is descheduled.
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &fmt, None).context("IAudioClient::Initialize")?;
            let event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture, event })
        }
    }

    /// Wait up to `timeout` for data, then hand every available packet to `f`.
    /// Returns false on timeout.
    pub fn read(&mut self, timeout: Duration, f: &mut dyn FnMut(Chunk)) -> Result<bool> {
        unsafe {
            let w = WaitForSingleObject(self.event, timeout.as_millis() as u32);
            let signalled = w == WAIT_OBJECT_0;
            loop {
                let n = self.capture.GetNextPacketSize()?;
                if n == 0 {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                let mut qpc = 0u64;
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc))?;
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                let samples = (!silent && !data.is_null())
                    .then(|| std::slice::from_raw_parts(data as *const f32, frames as usize * CHANNELS as usize));
                f(Chunk { qpc: qpc as i64, frames: frames as usize, samples });
                self.capture.ReleaseBuffer(frames)?;
            }
            Ok(signalled)
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}
