//! D3D11 device setup and GPU-side colour conversion / scaling.
//!
//! Conversion uses the D3D11 video processor (fixed-function on every vendor),
//! so BGRA -> NV12 + downscale happens on the GPU without custom shaders.

use std::collections::HashMap;
use std::mem::ManuallyDrop;

use anyhow::{anyhow, Context, Result};
use windows::core::Interface;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::{HMODULE, LUID, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

/// COM pointer wrapper for handing D3D objects between threads. The device is
/// created multithread-protected, so cross-thread use is sound.
pub struct SendPtr<T>(pub T);
unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

#[derive(Clone)]
pub struct Gpu {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub luid: u64,
    pub adapter_name: String,
}
unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

pub fn luid_u64(l: LUID) -> u64 {
    ((l.HighPart as u32 as u64) << 32) | l.LowPart as u64
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AdapterInfo {
    pub luid: u64,
    pub name: String,
    pub vendor_id: u32,
    pub dedicated_mb: u64,
}

pub fn adapters() -> Result<Vec<AdapterInfo>> {
    let mut out = Vec::new();
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(a) = factory.EnumAdapters1(i) {
            i += 1;
            let d = a.GetDesc1()?;
            if d.Flags & 2 != 0 {
                continue; // DXGI_ADAPTER_FLAG_SOFTWARE
            }
            out.push(AdapterInfo {
                luid: luid_u64(d.AdapterLuid),
                name: String::from_utf16_lossy(&d.Description).trim_end_matches('\0').to_string(),
                vendor_id: d.VendorId,
                dedicated_mb: d.DedicatedVideoMemory as u64 / (1024 * 1024),
            });
        }
    }
    Ok(out)
}

fn find_adapter(luid: u64) -> Result<Option<IDXGIAdapter1>> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(a) = factory.EnumAdapters1(i) {
            i += 1;
            if luid_u64(a.GetDesc1()?.AdapterLuid) == luid {
                return Ok(Some(a));
            }
        }
    }
    Ok(None)
}

impl Gpu {
    /// Create a device on the adapter with `luid`, or the default adapter.
    pub fn new(luid: Option<u64>) -> Result<Self> {
        let adapter = match luid {
            Some(l) => find_adapter(l)?,
            None => None,
        };
        let mut device = None;
        let mut context = None;
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
        unsafe {
            let adapter: Option<IDXGIAdapter> = adapter.map(|a| a.cast()).transpose()?;
            let driver = if adapter.is_some() { D3D_DRIVER_TYPE_UNKNOWN } else { D3D_DRIVER_TYPE_HARDWARE };
            D3D11CreateDevice(
                adapter.as_ref(),
                driver,
                HMODULE::default(),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .context("D3D11CreateDevice")?;
        }
        let device: ID3D11Device = device.ok_or_else(|| anyhow!("no device"))?;
        let context = context.ok_or_else(|| anyhow!("no context"))?;
        unsafe {
            // Required by Media Foundation, and lets capture/encode threads share it.
            let mt: ID3D11Multithread = device.cast()?;
            let _ = mt.SetMultithreadProtected(true);
        }
        let dxgi: IDXGIDevice = device.cast()?;
        let desc = unsafe { dxgi.GetAdapter()?.GetDesc()? };
        Ok(Self {
            device,
            context,
            luid: luid_u64(desc.AdapterLuid),
            adapter_name: String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string(),
        })
    }

    /// WinRT wrapper of the device, needed by Windows.Graphics.Capture.
    pub fn winrt_device(&self) -> Result<IDirect3DDevice> {
        let dxgi: IDXGIDevice = self.device.cast()?;
        let insp = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? };
        Ok(insp.cast()?)
    }

    pub fn texture(&self, w: u32, h: u32, format: DXGI_FORMAT, bind: D3D11_BIND_FLAG) -> Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
        tex.ok_or_else(|| anyhow!("CreateTexture2D returned null"))
    }

    pub fn staging(&self, w: u32, h: u32, format: DXGI_FORMAT) -> Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
        tex.ok_or_else(|| anyhow!("CreateTexture2D returned null"))
    }

    /// Copy a BGRA texture to CPU memory as tightly packed BGRA rows.
    pub fn read_bgra(&self, tex: &ID3D11Texture2D) -> Result<(u32, u32, Vec<u8>)> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut desc) };
        let staging = self.staging(desc.Width, desc.Height, desc.Format)?;
        unsafe {
            self.context.CopyResource(&staging, tex);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            let row = desc.Width as usize * 4;
            let mut out = vec![0u8; row * desc.Height as usize];
            for y in 0..desc.Height as usize {
                let src = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize), row);
                out[y * row..(y + 1) * row].copy_from_slice(src);
            }
            self.context.Unmap(&staging, 0);
            Ok((desc.Width, desc.Height, out))
        }
    }
}

/// BGRA (any size) -> NV12 (fixed output size), letterboxed to keep aspect.
pub struct Converter {
    gpu: Gpu,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    in_size: (u32, u32),
    pub out_size: (u32, u32),
    inputs: HashMap<usize, ID3D11VideoProcessorInputView>,
    outputs: Vec<(ID3D11Texture2D, ID3D11VideoProcessorOutputView)>,
    next: usize,
}
unsafe impl Send for Converter {}

/// Output textures in flight at once; the encoder releases a slot long before
/// it comes round again.
const OUTPUT_RING: usize = 8;

impl Converter {
    pub fn new(gpu: &Gpu, in_size: (u32, u32), out_size: (u32, u32), fps: u32) -> Result<Self> {
        let vdev: ID3D11VideoDevice = gpu.device.cast().context("video device")?;
        let vctx: ID3D11VideoContext = gpu.context.cast().context("video context")?;
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL { Numerator: fps, Denominator: 1 },
            InputWidth: in_size.0,
            InputHeight: in_size.1,
            OutputFrameRate: DXGI_RATIONAL { Numerator: fps, Denominator: 1 },
            OutputWidth: out_size.0,
            OutputHeight: out_size.1,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        let enumerator = unsafe { vdev.CreateVideoProcessorEnumerator(&desc)? };
        let processor = unsafe { vdev.CreateVideoProcessor(&enumerator, 0)? };

        let mut outputs = Vec::with_capacity(OUTPUT_RING);
        for _ in 0..OUTPUT_RING {
            let tex = gpu.texture(out_size.0, out_size.1, DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET)?;
            let vdesc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut view = None;
            unsafe { vdev.CreateVideoProcessorOutputView(&tex, &enumerator, &vdesc, Some(&mut view))? };
            outputs.push((tex, view.ok_or_else(|| anyhow!("no output view"))?));
        }

        let c = Self {
            gpu: gpu.clone(),
            vdev,
            vctx,
            enumerator,
            processor,
            in_size,
            out_size,
            inputs: HashMap::new(),
            outputs,
            next: 0,
        };
        c.configure();
        Ok(c)
    }

    fn configure(&self) {
        unsafe {
            let p = &self.processor;
            self.vctx.VideoProcessorSetStreamFrameFormat(p, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.vctx.VideoProcessorSetStreamAutoProcessingMode(p, 0, false);
            if let Ok(ctx1) = self.vctx.cast::<ID3D11VideoContext1>() {
                ctx1.VideoProcessorSetStreamColorSpace1(p, 0, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709);
                ctx1.VideoProcessorSetOutputColorSpace1(p, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709);
            }
            let black = D3D11_VIDEO_COLOR {
                Anonymous: D3D11_VIDEO_COLOR_0 { YCbCr: D3D11_VIDEO_COLOR_YCbCrA { Y: 0.0625, Cb: 0.5, Cr: 0.5, A: 1.0 } },
            };
            self.vctx.VideoProcessorSetOutputBackgroundColor(p, true, &black);

            // Letterbox: fit the source into the output keeping aspect ratio.
            let (iw, ih) = (self.in_size.0 as f64, self.in_size.1 as f64);
            let (ow, oh) = (self.out_size.0 as f64, self.out_size.1 as f64);
            let scale = (ow / iw).min(oh / ih);
            let (dw, dh) = ((iw * scale).round() as i32 & !1, (ih * scale).round() as i32 & !1);
            let dx = ((ow as i32 - dw) / 2) & !1;
            let dy = ((oh as i32 - dh) / 2) & !1;
            let src = RECT { left: 0, top: 0, right: self.in_size.0 as i32, bottom: self.in_size.1 as i32 };
            let dst = RECT { left: dx, top: dy, right: dx + dw, bottom: dy + dh };
            self.vctx.VideoProcessorSetStreamSourceRect(p, 0, true, Some(&src));
            self.vctx.VideoProcessorSetStreamDestRect(p, 0, true, Some(&dst));
        }
    }

    pub fn input_size(&self) -> (u32, u32) {
        self.in_size
    }

    /// Convert `src` into the next ring texture and return it.
    pub fn convert(&mut self, src: &ID3D11Texture2D) -> Result<ID3D11Texture2D> {
        let key = src.as_raw() as usize;
        if !self.inputs.contains_key(&key) {
            if self.inputs.len() > 16 {
                self.inputs.clear(); // frame pool was recreated; drop stale views
            }
            let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                },
            };
            let mut view = None;
            unsafe { self.vdev.CreateVideoProcessorInputView(src, &self.enumerator, &desc, Some(&mut view))? };
            self.inputs.insert(key, view.ok_or_else(|| anyhow!("no input view"))?);
        }
        let input = self.inputs[&key].clone();
        let (tex, out_view) = &self.outputs[self.next];
        self.next = (self.next + 1) % self.outputs.len();
        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: ManuallyDrop::new(Some(input)),
            ..Default::default()
        };
        let streams = [stream];
        let r = unsafe { self.vctx.VideoProcessorBlt(&self.processor, out_view, 0, &streams) };
        // Release the input view reference held by the stream struct.
        let [mut s] = streams;
        unsafe { ManuallyDrop::drop(&mut s.pInputSurface) };
        r.context("VideoProcessorBlt")?;
        Ok(tex.clone())
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }
}

/// Encoders want even dimensions (and some, multiples of 16 internally).
pub fn output_size(native: (u32, u32), target_height: u32) -> (u32, u32) {
    if target_height == 0 || target_height >= native.1 {
        return (native.0 & !1, native.1 & !1);
    }
    let w = (native.0 as f64 * target_height as f64 / native.1 as f64).round() as u32;
    (w & !1, target_height & !1)
}
