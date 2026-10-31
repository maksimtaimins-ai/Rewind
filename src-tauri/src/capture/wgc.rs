//! Windows.Graphics.Capture monitor capture. Frames arrive via the free-threaded
//! frame pool's callback (no polling); textures stay on the GPU.

use anyhow::{Context, Result};
use parking_lot::Mutex;
use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::CaptureSession;
use crate::gpu::{Gpu, SendPtr};

pub struct WgcSession {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    token: i64,
    stopped: bool,
}
unsafe impl Send for WgcSession {}

impl CaptureSession for WgcSession {
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let _ = self.pool.RemoveFrameArrived(self.token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

impl Drop for WgcSession {
    fn drop(&mut self) {
        self.stop();
    }
}

pub type FrameCallback = Box<dyn FnMut(&ID3D11Texture2D, i64) + Send>;

/// Start capturing `monitor` (an HMONITOR value). `on_frame` gets the frame
/// texture and its QPC timestamp in 100 ns units. Returns the session and the
/// monitor size.
pub fn start(gpu: &Gpu, monitor: isize, cursor: bool, on_frame: FrameCallback) -> Result<(WgcSession, (u32, u32))> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    let item: GraphicsCaptureItem =
        unsafe { interop.CreateForMonitor(HMONITOR(monitor as _)) }.context("CreateForMonitor")?;
    let size = item.Size()?;
    let device = gpu.winrt_device()?;
    let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device, format, 2, size)?;
    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetIsCursorCaptureEnabled(cursor);
    // Windows 11 only; older builds simply keep the yellow border.
    let _ = session.SetIsBorderRequired(false);

    let state = Mutex::new((on_frame, size));
    let device = SendPtr(device);
    let handler = TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(move |sender, _| {
        let device = &device; // capture the Send wrapper, not its field
        let pool = sender.ok()?;
        let frame = pool.TryGetNextFrame()?;
        let mut guard = state.lock();
        let (cb, last) = &mut *guard;
        let content = frame.ContentSize()?;
        if content.Width != last.Width || content.Height != last.Height {
            // Resolution changed: resize the pool. Next frame has the new size.
            *last = content;
            pool.Recreate(&device.0, format, 2, SizeInt32 { Width: content.Width, Height: content.Height })?;
            return Ok(());
        }
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        let tex: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let pts = frame.SystemRelativeTime()?.Duration;
        cb(&tex, pts);
        Ok(())
    });
    let token = pool.FrameArrived(&handler)?;
    session.StartCapture()?;
    Ok((WgcSession { session, pool, token, stopped: false }, (size.Width as u32, size.Height as u32)))
}
