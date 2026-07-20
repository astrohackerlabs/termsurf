use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use crate::ffi::RenderSurfaceExport;

type TsrcPort = u32;

pub struct RenderChannel {
    receive_port: TsrcPort,
    surface_send_port: TsrcPort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SentSurfaceMetadata {
    pub pixel_width: u64,
    pub pixel_height: u64,
    pub bytes_per_row: u64,
    pub pixel_format: u32,
    pub generation: u64,
    pub attachment_id: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct TsrcSurfaceMetadata {
    width: c_uint,
    height: c_uint,
    bytes_per_row: c_uint,
    pixel_format: c_uint,
    generation: c_uint,
    attachment_id: u64,
    imported_width: c_uint,
    imported_height: c_uint,
    imported_bytes_per_row: c_uint,
    imported_pixel_format: c_uint,
}

static GLOBAL_RENDER_CHANNEL: OnceLock<Mutex<Option<RenderChannel>>> = OnceLock::new();
static NEXT_ATTACHMENT_ID: AtomicU64 = AtomicU64::new(1);

impl Drop for RenderChannel {
    fn drop(&mut self) {
        if self.surface_send_port != 0 {
            unsafe { tsrc_deallocate_port(self.surface_send_port) };
            self.surface_send_port = 0;
        }
        if self.receive_port != 0 {
            unsafe { tsrc_destroy_receive_port(self.receive_port) };
            self.receive_port = 0;
        }
    }
}

pub fn connect(service_name: &str) -> Option<RenderChannel> {
    connect_impl(service_name)
}

pub fn connect_global(service_name: &str) -> bool {
    let Some(channel) = connect(service_name) else {
        return false;
    };
    let mutex = GLOBAL_RENDER_CHANNEL.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().expect("render channel mutex poisoned");
    *guard = Some(channel);
    true
}

pub fn send_exported_surface_global(exported: RenderSurfaceExport) -> Option<SentSurfaceMetadata> {
    let mutex = GLOBAL_RENDER_CHANNEL.get()?;
    let mut guard = mutex.lock().ok()?;
    let channel = guard.as_mut()?;
    channel.send_exported_surface(exported)
}

#[cfg(target_os = "macos")]
fn connect_impl(service_name: &str) -> Option<RenderChannel> {
    let service = match CString::new(service_name) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("[Ladybird] render side-channel skipped: service name contains NUL");
            return None;
        }
    };

    let mut receive_port: TsrcPort = 0;
    let result = unsafe {
        tsrc_child_connect_and_send(
            service.as_ptr(),
            TSRC_DEFAULT_TIMEOUT_MS,
            &mut receive_port as *mut TsrcPort,
        )
    };
    if result == TSRC_OK {
        eprintln!(
            "[Ladybird] render side-channel handshake sent service={service_name} receive_port={receive_port}"
        );
        let mut channel = RenderChannel {
            receive_port,
            surface_send_port: 0,
        };
        if channel.wait_for_surface_receiver() {
            Some(channel)
        } else {
            None
        }
    } else {
        eprintln!(
            "[Ladybird] render side-channel handshake failed service={service_name} result={}",
            result_name(result)
        );
        None
    }
}

impl RenderChannel {
    #[cfg(target_os = "macos")]
    fn wait_for_surface_receiver(&mut self) -> bool {
        let mut surface_send_port: TsrcPort = 0;
        let result = unsafe {
            tsrc_wait_for_surface_receiver(
                self.receive_port,
                TSRC_DEFAULT_TIMEOUT_MS,
                &mut surface_send_port as *mut TsrcPort,
            )
        };
        if result != TSRC_OK {
            eprintln!(
                "[Ladybird] render side-channel receiver wait failed result={}",
                result_name(result)
            );
            return false;
        }
        self.surface_send_port = surface_send_port;
        eprintln!(
            "[Ladybird] render side-channel surface receiver ready send_port={surface_send_port}"
        );
        true
    }

    #[cfg(target_os = "macos")]
    pub fn send_exported_surface(
        &mut self,
        exported: RenderSurfaceExport,
    ) -> Option<SentSurfaceMetadata> {
        if self.surface_send_port == 0 {
            eprintln!("[Ladybird] render side-channel has no surface receiver send port");
            deallocate_exported_surface_port(exported);
            return None;
        }
        if !usable_export(exported) {
            eprintln!(
                "[Ladybird] render side-channel export is not transferable export={exported:?}"
            );
            deallocate_exported_surface_port(exported);
            return None;
        }
        let attachment_id = NEXT_ATTACHMENT_ID.fetch_add(1, Ordering::Relaxed);
        let result = unsafe {
            tsrc_send_surface(
                self.surface_send_port,
                exported.surface_port,
                exported.pixel_width as c_uint,
                exported.pixel_height as c_uint,
                exported.bytes_per_row as c_uint,
                exported.pixel_format,
                exported.generation as c_uint,
                attachment_id,
                TSRC_DEFAULT_TIMEOUT_MS,
            )
        };
        if result == TSRC_OK {
            eprintln!(
                "[Ladybird] render side-channel real surface sent width={} height={} generation={} attachment_id={attachment_id}",
                exported.pixel_width, exported.pixel_height, exported.generation
            );
            Some(SentSurfaceMetadata {
                pixel_width: exported.pixel_width as u64,
                pixel_height: exported.pixel_height as u64,
                bytes_per_row: exported.bytes_per_row as u64,
                pixel_format: exported.pixel_format,
                generation: exported.generation as u64,
                attachment_id,
            })
        } else {
            eprintln!(
                "[Ladybird] render side-channel real surface failed result={}",
                result_name(result)
            );
            None
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn wait_for_surface_receiver(&mut self) -> bool {
        false
    }

    #[cfg(not(target_os = "macos"))]
    pub fn send_exported_surface(
        &mut self,
        _exported: RenderSurfaceExport,
    ) -> Option<SentSurfaceMetadata> {
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn connect_impl(service_name: &str) -> Option<RenderChannel> {
    eprintln!("[Ladybird] render side-channel unsupported on this platform service={service_name}");
    None
}

fn result_name(result: c_int) -> String {
    let ptr = unsafe { tsrc_result_name(result) };
    if ptr.is_null() {
        return "unknown".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "macos")]
pub fn real_frame_attachment_smoke(exported: RenderSurfaceExport) -> bool {
    if !usable_export(exported) {
        eprintln!("[Ladybird] real-frame-attachment-smoke invalid export={exported:?}");
        deallocate_exported_surface_port(exported);
        return false;
    }

    let service_name = format!(
        "com.astrohacker.terminal.ladybirdd.real-frame-smoke.{}",
        std::process::id()
    );
    let service = match CString::new(service_name.clone()) {
        Ok(service) => service,
        Err(_) => {
            eprintln!("[Ladybird] real-frame-attachment-smoke service name contains NUL");
            deallocate_exported_surface_port(exported);
            return false;
        }
    };

    let mut control_port: TsrcPort = 0;
    let result = unsafe { tsrc_register_service(service.as_ptr(), &mut control_port) };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke register failed result={}",
            result_name(result)
        );
        deallocate_exported_surface_port(exported);
        return false;
    }

    let receiver = thread::spawn(move || receive_one_surface(control_port));

    let mut receive_port: TsrcPort = 0;
    let result = unsafe {
        tsrc_child_connect_and_send(
            service.as_ptr(),
            TSRC_DEFAULT_TIMEOUT_MS,
            &mut receive_port as *mut TsrcPort,
        )
    };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke child connect failed result={}",
            result_name(result)
        );
        deallocate_exported_surface_port(exported);
        let _ = receiver.join();
        return false;
    }

    let mut surface_send_port: TsrcPort = 0;
    let result = unsafe {
        tsrc_wait_for_surface_receiver(
            receive_port,
            TSRC_DEFAULT_TIMEOUT_MS,
            &mut surface_send_port as *mut TsrcPort,
        )
    };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke surface receiver wait failed result={}",
            result_name(result)
        );
        deallocate_exported_surface_port(exported);
        unsafe { tsrc_destroy_receive_port(receive_port) };
        let _ = receiver.join();
        return false;
    }

    let attachment_id = 1_u64;
    let result = unsafe {
        tsrc_send_surface(
            surface_send_port,
            exported.surface_port,
            exported.pixel_width as c_uint,
            exported.pixel_height as c_uint,
            exported.bytes_per_row as c_uint,
            exported.pixel_format,
            exported.generation as c_uint,
            attachment_id,
            TSRC_DEFAULT_TIMEOUT_MS,
        )
    };
    unsafe {
        tsrc_deallocate_port(surface_send_port);
        tsrc_destroy_receive_port(receive_port);
    }
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke surface send failed result={}",
            result_name(result)
        );
        let _ = receiver.join();
        return false;
    }

    let metadata = match receiver.join() {
        Ok(Some(metadata)) => metadata,
        Ok(None) => return false,
        Err(_) => {
            eprintln!("[Ladybird] real-frame-attachment-smoke receiver thread panicked");
            return false;
        }
    };

    let ok = metadata.width == exported.pixel_width as c_uint
        && metadata.height == exported.pixel_height as c_uint
        && metadata.bytes_per_row == exported.bytes_per_row as c_uint
        && metadata.pixel_format == exported.pixel_format
        && metadata.generation == exported.generation as c_uint
        && metadata.attachment_id == attachment_id
        && metadata.imported_width == exported.pixel_width as c_uint
        && metadata.imported_height == exported.pixel_height as c_uint
        && metadata.imported_bytes_per_row == exported.bytes_per_row as c_uint
        && metadata.imported_pixel_format == exported.pixel_format;

    eprintln!("[Ladybird] real-frame-attachment-smoke imported metadata={metadata:?} matched={ok}");
    if ok {
        eprintln!("[Ladybird] real-frame-attachment-smoke PASS attachment_id={attachment_id}");
    }
    ok
}

#[cfg(not(target_os = "macos"))]
pub fn real_frame_attachment_smoke(_exported: RenderSurfaceExport) -> bool {
    eprintln!("[Ladybird] real-frame-attachment-smoke unsupported on this platform");
    false
}

#[cfg(target_os = "macos")]
fn receive_one_surface(control_port: TsrcPort) -> Option<TsrcSurfaceMetadata> {
    let mut child_port: TsrcPort = 0;
    let result =
        unsafe { tsrc_wait_for_child_port(control_port, TSRC_DEFAULT_TIMEOUT_MS, &mut child_port) };
    unsafe { tsrc_destroy_receive_port(control_port) };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke receiver child wait failed result={}",
            result_name(result)
        );
        return None;
    }

    let mut surface_receive_port: TsrcPort = 0;
    let result = unsafe {
        tsrc_send_surface_receiver(
            child_port,
            TSRC_DEFAULT_TIMEOUT_MS,
            &mut surface_receive_port,
        )
    };
    unsafe { tsrc_deallocate_port(child_port) };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke send receiver failed result={}",
            result_name(result)
        );
        return None;
    }

    let mut metadata = TsrcSurfaceMetadata::default();
    let result = unsafe {
        tsrc_receive_test_surface(surface_receive_port, TSRC_DEFAULT_TIMEOUT_MS, &mut metadata)
    };
    unsafe { tsrc_destroy_receive_port(surface_receive_port) };
    if result != TSRC_OK {
        eprintln!(
            "[Ladybird] real-frame-attachment-smoke receive surface failed result={}",
            result_name(result)
        );
        return None;
    }
    Some(metadata)
}

fn usable_export(exported: RenderSurfaceExport) -> bool {
    exported.has_surface
        && exported.surface_port != 0
        && exported.pixel_width > 0
        && exported.pixel_height > 0
        && exported.bytes_per_row > 0
        && exported.pixel_format != 0
        && exported.generation > 0
}

fn deallocate_exported_surface_port(exported: RenderSurfaceExport) {
    if exported.surface_port != 0 {
        unsafe { tsrc_deallocate_port(exported.surface_port) };
    }
}

const TSRC_OK: c_int = 0;
const TSRC_DEFAULT_TIMEOUT_MS: c_uint = 1000;

extern "C" {
    fn tsrc_register_service(service_name: *const c_char, out_control_port: *mut TsrcPort)
        -> c_int;
    fn tsrc_wait_for_child_port(
        control_port: TsrcPort,
        timeout_ms: c_uint,
        out_child_port: *mut TsrcPort,
    ) -> c_int;
    fn tsrc_child_connect_and_send(
        service_name: *const c_char,
        timeout_ms: c_uint,
        out_receive_port: *mut TsrcPort,
    ) -> c_int;
    fn tsrc_wait_for_surface_receiver(
        child_receive_port: TsrcPort,
        timeout_ms: c_uint,
        out_surface_send_port: *mut TsrcPort,
    ) -> c_int;
    fn tsrc_send_surface_receiver(
        child_port: TsrcPort,
        timeout_ms: c_uint,
        out_surface_receive_port: *mut TsrcPort,
    ) -> c_int;
    fn tsrc_send_surface(
        surface_send_port: TsrcPort,
        exported_surface_port: TsrcPort,
        width: c_uint,
        height: c_uint,
        bytes_per_row: c_uint,
        pixel_format: c_uint,
        generation: c_uint,
        attachment_id: u64,
        timeout_ms: c_uint,
    ) -> c_int;
    fn tsrc_receive_test_surface(
        surface_receive_port: TsrcPort,
        timeout_ms: c_uint,
        out_metadata: *mut TsrcSurfaceMetadata,
    ) -> c_int;
    fn tsrc_deallocate_port(port: TsrcPort);
    fn tsrc_destroy_receive_port(port: TsrcPort);
    fn tsrc_result_name(result: c_int) -> *const c_char;
}
