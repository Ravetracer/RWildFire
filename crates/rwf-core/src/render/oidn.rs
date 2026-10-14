// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Intel Open Image Denoise (OIDN 2.x), loaded at runtime: RWildFire neither links it nor
//! needs it to build or run. The library is searched in `RWF_OIDN_DIR`, next to the
//! executable (`lib/` on Linux and macOS, the executable's folder on Windows), in `oidn/` of
//! the source tree (filled by `tools/fetch_oidn.py`) and finally by the system's library
//! search. Devices are created once and kept.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use libloading::Library;

/// Environment variable with a folder that contains the OIDN library.
pub const DIR_ENV: &str = "RWF_OIDN_DIR";

#[cfg(target_os = "linux")]
const LIB_NAME: &str = "libOpenImageDenoise.so.2";
#[cfg(target_os = "macos")]
const LIB_NAME: &str = "libOpenImageDenoise.2.dylib";
#[cfg(windows)]
const LIB_NAME: &str = "OpenImageDenoise.dll";

type Device = *mut c_void;
type Buffer = *mut c_void;
type Filter = *mut c_void;

/// `OIDNDeviceType`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// The best device available (a supported GPU, else the CPU).
    Default = 0,
    Cpu = 1,
    Sycl = 2,
    Cuda = 3,
    Hip = 4,
    Metal = 5,
}

impl DeviceType {
    fn from_raw(v: i32) -> Option<Self> {
        [Self::Default, Self::Cpu, Self::Sycl, Self::Cuda, Self::Hip, Self::Metal].into_iter().find(|d| *d as i32 == v)
    }

    pub fn label(self) -> &'static str {
        match self {
            DeviceType::Default => "default",
            DeviceType::Cpu => "CPU",
            DeviceType::Sycl => "Intel GPU",
            DeviceType::Cuda => "NVIDIA GPU",
            DeviceType::Hip => "AMD GPU",
            DeviceType::Metal => "Apple GPU",
        }
    }
}

const FORMAT_FLOAT3: i32 = 3;

struct Api {
    new_device: unsafe extern "C" fn(i32) -> Device,
    commit_device: unsafe extern "C" fn(Device),
    get_device_int: unsafe extern "C" fn(Device, *const c_char) -> i32,
    get_device_error: unsafe extern "C" fn(Device, *mut *const c_char) -> i32,
    new_buffer: unsafe extern "C" fn(Device, usize) -> Buffer,
    write_buffer: unsafe extern "C" fn(Buffer, usize, usize, *const c_void),
    read_buffer: unsafe extern "C" fn(Buffer, usize, usize, *mut c_void),
    release_buffer: unsafe extern "C" fn(Buffer),
    new_filter: unsafe extern "C" fn(Device, *const c_char) -> Filter,
    release_filter: unsafe extern "C" fn(Filter),
    set_filter_image: unsafe extern "C" fn(Filter, *const c_char, Buffer, i32, usize, usize, usize, usize, usize),
    set_filter_bool: unsafe extern "C" fn(Filter, *const c_char, bool),
    commit_filter: unsafe extern "C" fn(Filter),
    execute_filter: unsafe extern "C" fn(Filter),
}

/// The loaded library with its devices (by requested type).
pub struct Oidn {
    api: Api,
    path: PathBuf,
    devices: HashMap<DeviceType, Result<Device, String>>,
    // Keeps the functions valid; dropped last.
    _lib: Library,
}

// The OIDN API is thread-safe; access is serialized by the mutex around `Oidn` anyway.
unsafe impl Send for Oidn {}

/// Folders searched for the library, in order.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os(DIR_ENV) {
        dirs.push(PathBuf::from(d));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.clone());
        // Development: target/<profile>/<exe> and target/<profile>/deps/<test>.
        for up in [2, 3] {
            if let Some(root) = exe_dir.ancestors().nth(up) {
                dirs.push(root.join("oidn"));
            }
        }
    }
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../oidn")));
    dirs
}

fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in search_dirs() {
        for p in [d.join("lib").join(LIB_NAME), d.join(LIB_NAME)] {
            if p.is_file() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(windows)]
unsafe fn open(path: &Path) -> Result<Library, libloading::Error> {
    // Let the DLL find its sibling DLLs (core, devices, tbb) in its own folder.
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;
    unsafe { libloading::os::windows::Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH).map(Into::into) }
}

#[cfg(not(windows))]
unsafe fn open(path: &Path) -> Result<Library, libloading::Error> {
    unsafe { Library::new(path) }
}

impl Oidn {
    fn load() -> Result<Oidn, String> {
        let mut errors = Vec::new();
        let mut tried: Vec<PathBuf> = candidates();
        tried.push(PathBuf::from(LIB_NAME));
        for path in tried {
            // SAFETY: loading OIDN runs its library constructors only.
            match unsafe { open(&path) } {
                Ok(lib) => match unsafe { Self::bind(lib, path.clone()) } {
                    Ok(o) => return Ok(o),
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                },
                Err(e) if path.is_absolute() => errors.push(format!("{}: {e}", path.display())),
                Err(_) => {}
            }
        }
        if errors.is_empty() { Err("Open Image Denoise not found".into()) } else { Err(errors.join("; ")) }
    }

    unsafe fn bind(lib: Library, path: PathBuf) -> Result<Oidn, String> {
        macro_rules! sym {
            ($name:literal) => {
                *unsafe { lib.get(concat!($name, "\0").as_bytes()) }.map_err(|e| format!("{}: {e}", $name))?
            };
        }
        let api = Api {
            new_device: sym!("oidnNewDevice"),
            commit_device: sym!("oidnCommitDevice"),
            get_device_int: sym!("oidnGetDeviceInt"),
            get_device_error: sym!("oidnGetDeviceError"),
            new_buffer: sym!("oidnNewBuffer"),
            write_buffer: sym!("oidnWriteBuffer"),
            read_buffer: sym!("oidnReadBuffer"),
            release_buffer: sym!("oidnReleaseBuffer"),
            new_filter: sym!("oidnNewFilter"),
            release_filter: sym!("oidnReleaseFilter"),
            set_filter_image: sym!("oidnSetFilterImage"),
            set_filter_bool: sym!("oidnSetFilterBool"),
            commit_filter: sym!("oidnCommitFilter"),
            execute_filter: sym!("oidnExecuteFilter"),
        };
        Ok(Oidn { api, path, devices: HashMap::new(), _lib: lib })
    }

    /// The last error of `device` (clears it).
    fn error(&self, device: Device) -> Option<String> {
        let mut msg: *const c_char = std::ptr::null();
        let code = unsafe { (self.api.get_device_error)(device, &mut msg) };
        (code != 0).then(|| {
            let text = if msg.is_null() { String::new() } else { unsafe { CStr::from_ptr(msg) }.to_string_lossy().into_owned() };
            format!("OIDN error {code}: {text}")
        })
    }

    fn device(&mut self, ty: DeviceType) -> Result<Device, String> {
        if let Some(d) = self.devices.get(&ty) {
            return d.clone();
        }
        let d = unsafe { (self.api.new_device)(ty as i32) };
        let r = if d.is_null() {
            // A failed creation reports through the null device.
            Err(self.error(std::ptr::null_mut()).unwrap_or_else(|| format!("no {} device", ty.label())))
        } else {
            unsafe { (self.api.commit_device)(d) };
            match self.error(d) {
                Some(e) => Err(e),
                None => Ok(d),
            }
        };
        self.devices.insert(ty, r.clone());
        r
    }

    /// The device type actually behind `device`.
    fn device_type(&self, device: Device) -> DeviceType {
        let name = CString::new("type").unwrap();
        DeviceType::from_raw(unsafe { (self.api.get_device_int)(device, name.as_ptr()) }).unwrap_or(DeviceType::Default)
    }

    /// Denoises `color` (RGB, `width` × `height`, top to bottom) in place with the `RT`
    /// filter. `hdr`: linear values ≥ 0, otherwise 0..1 (`srgb`: sRGB/gamma encoded).
    /// Returns the device type used.
    pub fn denoise(
        &mut self,
        color: &mut [f32],
        width: usize,
        height: usize,
        hdr: bool,
        srgb: bool,
        prefer: DeviceType,
    ) -> Result<DeviceType, String> {
        if color.len() != width * height * 3 {
            return Err("image size mismatch".into());
        }
        let device = match self.device(prefer) {
            Ok(d) => d,
            Err(_) if prefer != DeviceType::Cpu => self.device(DeviceType::Cpu)?,
            Err(e) => return Err(e),
        };
        let bytes = std::mem::size_of_val(color);
        let api = &self.api;
        let c = |s: &str| CString::new(s).unwrap();
        unsafe {
            let input = (api.new_buffer)(device, bytes);
            let output = (api.new_buffer)(device, bytes);
            let filter = (api.new_filter)(device, c("RT").as_ptr());
            let result = (|| {
                if input.is_null() || output.is_null() || filter.is_null() {
                    return Err(self.error(device).unwrap_or_else(|| "OIDN: out of memory".into()));
                }
                (api.write_buffer)(input, 0, bytes, color.as_ptr().cast());
                (api.set_filter_image)(filter, c("color").as_ptr(), input, FORMAT_FLOAT3, width, height, 0, 0, 0);
                (api.set_filter_image)(filter, c("output").as_ptr(), output, FORMAT_FLOAT3, width, height, 0, 0, 0);
                (api.set_filter_bool)(filter, c("hdr").as_ptr(), hdr);
                (api.set_filter_bool)(filter, c("srgb").as_ptr(), srgb && !hdr);
                (api.commit_filter)(filter);
                if let Some(e) = self.error(device) {
                    return Err(e);
                }
                (api.execute_filter)(filter);
                if let Some(e) = self.error(device) {
                    return Err(e);
                }
                (api.read_buffer)(output, 0, bytes, color.as_mut_ptr().cast());
                self.error(device).map_or(Ok(()), Err)
            })();
            if !filter.is_null() {
                (api.release_filter)(filter);
            }
            for b in [input, output] {
                if !b.is_null() {
                    (api.release_buffer)(b);
                }
            }
            result?;
        }
        Ok(self.device_type(device))
    }

    /// Path of the loaded library.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

static OIDN: OnceLock<Mutex<Result<Oidn, String>>> = OnceLock::new();

fn global() -> &'static Mutex<Result<Oidn, String>> {
    OIDN.get_or_init(|| Mutex::new(Oidn::load()))
}

/// Path of the OIDN library, or why it is not available.
pub fn library() -> Result<PathBuf, String> {
    match &*global().lock().unwrap_or_else(|e| e.into_inner()) {
        Ok(o) => Ok(o.path().to_path_buf()),
        Err(e) => Err(e.clone()),
    }
}

/// [`Oidn::denoise`] with the shared library instance.
pub fn denoise(color: &mut [f32], width: usize, height: usize, hdr: bool, srgb: bool, prefer: DeviceType) -> Result<DeviceType, String> {
    match &mut *global().lock().unwrap_or_else(|e| e.into_inner()) {
        Ok(o) => o.denoise(color, width, height, hdr, srgb, prefer),
        Err(e) => Err(e.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth image with strong per-pixel noise.
    fn noisy(w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
        let mut clean = Vec::with_capacity(w * h * 3);
        let mut state = 0x1234_5678u32;
        let mut rnd = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32
        };
        let mut noisy = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let v = [x as f32 / w as f32, y as f32 / h as f32, 0.5];
                for c in v {
                    clean.push(c);
                    noisy.push((c + (rnd() - 0.5) * 0.4).clamp(0.0, 1.0));
                }
            }
        }
        (clean, noisy)
    }

    fn rms(a: &[f32], b: &[f32]) -> f32 {
        (a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>() / a.len() as f32).sqrt()
    }

    #[test]
    fn denoises_when_available() {
        if let Err(e) = library() {
            eprintln!("OIDN not available, skipped ({e})");
            return;
        }
        let (w, h) = (96, 64);
        let (clean, noisy) = noisy(w, h);
        for prefer in [DeviceType::Cpu, DeviceType::Default, DeviceType::Cuda] {
            let mut img = noisy.clone();
            match denoise(&mut img, w, h, false, false, prefer) {
                Ok(used) => {
                    let (before, after) = (rms(&noisy, &clean), rms(&img, &clean));
                    eprintln!("{prefer:?} -> {used:?}: rms {before:.3} -> {after:.3}");
                    assert!(after < before * 0.5, "{prefer:?}: noise not reduced ({before} -> {after})");
                }
                Err(e) => panic!("{prefer:?}: {e}"),
            }
        }
    }
}
