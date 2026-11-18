pub mod abi;

use abi::*;
use core::ffi::c_void;
use libloading::Library;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread::{self, ThreadId};

// The engine is built for the game's system (the Windows one with the GNU toolchain: a plain C ABI DLL) and
// named the way that system names a shared library.
#[cfg(windows)]
pub const ENGINE_FILE: &str = "sunset_baritone_engine.dll";
#[cfg(not(windows))]
pub const ENGINE_FILE: &str = "libsunset_baritone_engine.so";
pub const ENGINE_VERSION: &str = "1.1.52";

// Option<extern fn> has the C nullable-function-pointer representation. Receive
// into this always-valid view, then construct the frozen non-null table only
// after all required callbacks have been checked.
// https://doc.rust-lang.org/nomicon/ffi.html#the-nullable-pointer-optimization
#[repr(C)]
#[derive(Clone, Copy)]
struct RawApiV1 {
    abi: u32,
    size: u32,
    create: Option<unsafe extern "C" fn() -> *mut c_void>,
    destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    dispatch: Option<
        unsafe extern "C" fn(*mut c_void, u32, BytesV1, *const HostV1, EmitV1, *mut c_void) -> i32,
    >,
}
const _: () = {
    assert!(size_of::<RawApiV1>() == size_of::<ApiV1>());
    assert!(align_of::<RawApiV1>() == align_of::<ApiV1>());
};
impl RawApiV1 {
    fn empty() -> Self {
        Self {
            abi: 0,
            size: 0,
            create: None,
            destroy: None,
            dispatch: None,
        }
    }
    fn checked(self) -> Result<ApiV1, String> {
        if self.abi != ABI_V1 || self.size != size_of::<ApiV1>() as u32 {
            return Err("Baritone DLL ABI/table size is incompatible".into());
        }
        let (Some(create), Some(destroy), Some(dispatch)) =
            (self.create, self.destroy, self.dispatch)
        else {
            return Err("Baritone DLL table is missing a required callback".into());
        };
        Ok(ApiV1 {
            abi: self.abi,
            size: self.size,
            create,
            destroy,
            dispatch,
        })
    }
}

// Source-inserted game modules cannot use include_bytes at this transformer pin.
// Only this independent crate includes the staged payload, as opaque data.
#[cfg(all(feature = "bundled-engine", windows))]
pub fn bundled_engine_bytes() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../native-data/sunset_baritone/engine/x86_64-pc-windows-gnu/sunset_baritone_engine.dll"
    ))
}
#[cfg(all(feature = "bundled-engine", not(windows)))]
pub fn bundled_engine_bytes() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../native-data/sunset_baritone/engine/x86_64-unknown-linux-gnu/libsunset_baritone_engine.so"
    ))
}

/// Independent read-only game adapter. No reference survives Runtime::call.
pub trait Host {
    fn block(&self, pos: [i32; 3]) -> Result<BlockReadV1, String>;
    fn query(&self, operation: u32, request: &[u8]) -> Result<Vec<u8>, String>;
}

/// Prefer a compatible user replacement. Both replacement and extracted stock
/// files are preserved, including modified existing stock extractions.
pub fn engine_path(root: &Path, bundled: &[u8]) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("Baritone data path must be absolute".into());
    }
    let override_path = root.join("engine-override").join(ENGINE_FILE);
    if override_path.try_exists().map_err(|e| e.to_string())? {
        return Ok(override_path);
    }
    if bundled.is_empty() {
        return Err("Baritone engine payload is missing".into());
    }
    let hash = bundled.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    let dir = root
        .join("engine")
        .join(ENGINE_VERSION)
        .join(format!("{hash:016x}"));
    fs::create_dir_all(&dir).map_err(|e| format!("Baritone engine directory: {e}"))?;
    let path = dir.join(ENGINE_FILE);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file
            .write_all(bundled)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("Baritone engine extraction: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("Baritone engine extraction: {e}")),
    }
    Ok(path)
}

#[cfg(windows)]
fn check_architecture(path: &Path) -> Result<(), String> {
    use std::io::{Seek, SeekFrom};
    let mut file =
        fs::File::open(path).map_err(|e| format!("Baritone engine {}: {e}", path.display()))?;
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| "Baritone engine library is not a Windows PE library")?;
    if &dos[..2] != b"MZ" {
        return Err("Baritone engine library is not a Windows PE library".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().expect("four-byte slice"));
    if offset > 16 * 1024 * 1024 {
        return Err("Baritone engine PE offset is invalid".into());
    }
    file.seek(SeekFrom::Start(u64::from(offset)))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 24];
    file.read_exact(&mut pe)
        .map_err(|_| "Baritone engine PE header is truncated")?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664 {
        return Err("Baritone engine must be x86_64, matching the game".into());
    }
    if u16::from_le_bytes([pe[22], pe[23]]) & 0x2000 == 0 {
        return Err("Baritone engine is not a DLL".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn check_architecture(path: &Path) -> Result<(), String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Baritone engine {}: {e}", path.display()))?;
    let mut elf = [0u8; 20];
    file.read_exact(&mut elf)
        .map_err(|_| "Baritone engine library is not an ELF library")?;
    if &elf[..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
        return Err("Baritone engine library is not an ELF library".into());
    }
    if u16::from_le_bytes([elf[18], elf[19]]) != 0x3e {
        return Err("Baritone engine must be x86_64, matching the game".into());
    }
    if u16::from_le_bytes([elf[16], elf[17]]) != 3 {
        return Err("Baritone engine is not a shared library".into());
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn load(path: &Path) -> Result<Library, libloading::Error> {
    unsafe { Library::new(path) }
}

// Never unmapped on Linux (RTLD_NODELETE, glibc's 0x1000): the engine's thread-local destructors stay registered
// with the threads that called it, and a thread ending after an unload would run code that is gone.
#[cfg(not(windows))]
unsafe fn load(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::unix;
    const RTLD_NODELETE: std::os::raw::c_int = 0x1000;
    unsafe { unix::Library::open(Some(path), unix::RTLD_NOW | unix::RTLD_LOCAL | RTLD_NODELETE) }.map(Library::from)
}

/// The library outlives its instance/function table. Rc marker and creator thread
/// checks deliberately prohibit Send/Sync and cross-thread callback use.
pub struct Runtime {
    api: ApiV1,
    engine: *mut c_void,
    creator: ThreadId,
    _thread_confined: PhantomData<Rc<()>>,
    _library: Library,
}

impl Runtime {
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Baritone DLL path must be absolute".into());
        }
        if !cfg!(all(any(windows, target_os = "linux"), target_arch = "x86_64")) {
            return Err("Baritone engine V1 runs on x86_64 Windows and Linux".into());
        }
        check_architecture(path)?;
        // SAFETY: absolute user-selected or bundled library; architecture checked.
        // The entry point and callback contract is ABI v1. An incompatible native
        // library can still violate that contract; it is executable code.
        let library = unsafe { load(path) }.map_err(|e| format!("Baritone engine load: {e}"))?;
        let discover = unsafe { library.get::<DiscoverV1>(b"horizon_baritone_api_v1\0") }
            .map_err(|e| format!("Baritone DLL discovery: {e}"))?;
        let mut output = RawApiV1::empty();
        let code = unsafe {
            discover(
                (&mut output as *mut RawApiV1).cast(),
                size_of::<ApiV1>() as u32,
            )
        };
        if code != OK {
            return Err(format!("Baritone DLL discovery failed ({code})"));
        }
        let api = output.checked()?;
        let engine = unsafe { (api.create)() };
        if engine.is_null() {
            return Err("Baritone DLL could not create an engine".into());
        }
        let mut runtime = Self {
            api,
            engine,
            creator: thread::current().id(),
            _thread_confined: PhantomData,
            _library: library,
        };
        let reply = runtime.call(7, b"{\"schema\":1}", None)?;
        let value: serde_json::Value = serde_json::from_slice(&reply)
            .map_err(|_| "Baritone DLL capability response is invalid")?;
        let caps = value["capabilities"]
            .as_array()
            .ok_or("Baritone DLL has no capability declaration")?;
        for required in ["goto", "visible_mine", "predicted_mine"] {
            if !caps.iter().any(|v| v.as_str() == Some(required)) {
                return Err(format!("Baritone DLL lacks {required}"));
            }
        }
        Ok(runtime)
    }
    pub fn call(
        &mut self,
        operation: u32,
        request: &[u8],
        host: Option<&dyn Host>,
    ) -> Result<Vec<u8>, String> {
        if thread::current().id() != self.creator {
            return Err("Baritone engine call is on the wrong thread".into());
        }
        if request.len() > MAX_BYTES {
            return Err("Baritone request exceeds 4 MiB".into());
        }
        let mut bridge = Bridge { host };
        let host_v1 = HostV1 {
            abi: ABI_V1,
            size: size_of::<HostV1>() as u32,
            user: (&mut bridge as *mut Bridge<'_>).cast(),
            block: read_block,
            query: read_query,
        };
        let mut output = Output::default();
        let code = unsafe {
            (self.api.dispatch)(
                self.engine,
                operation,
                BytesV1 {
                    ptr: request.as_ptr(),
                    len: request.len() as u32,
                },
                if host.is_some() {
                    &host_v1
                } else {
                    std::ptr::null()
                },
                collect,
                (&mut output as *mut Output).cast(),
            )
        };
        if code != OK {
            return Err(format!(
                "Baritone engine operation {operation} failed ({code})"
            ));
        }
        if output.calls != 1 {
            return Err("Baritone engine did not emit one complete response".into());
        }
        let bytes = output.bytes.ok_or("Baritone response was not copied")?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "Baritone response is not valid JSON")?;
        if value["schema"].as_u64() != Some(1) {
            return Err("Baritone response schema is incompatible".into());
        }
        Ok(bytes)
    }
    pub fn cancel(&mut self) -> Result<(), String> {
        self.call(6, b"{\"schema\":1,\"reason\":\"host stop\"}", None)
            .map(|_| ())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: instance came from this table and every synchronous call has
        // returned; _library is still alive while destroy runs.
        unsafe { (self.api.destroy)(self.engine) };
        self.engine = std::ptr::null_mut();
    }
}

struct Bridge<'a> {
    host: Option<&'a dyn Host>,
}
#[derive(Default)]
struct Output {
    bytes: Option<Vec<u8>>,
    calls: usize,
}

unsafe fn borrowed<'a>(bytes: BytesV1) -> Result<&'a [u8], i32> {
    if bytes.len as usize > MAX_BYTES || (bytes.len != 0 && bytes.ptr.is_null()) {
        return Err(BAD_REQUEST);
    }
    if bytes.len == 0 {
        return Ok(&[]);
    }
    // SAFETY: caller guarantees a readable buffer for the callback's duration.
    Ok(unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len as usize) })
}

unsafe extern "C" fn collect(user: *mut c_void, bytes: BytesV1) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if user.is_null() {
            return BAD_REQUEST;
        }
        let output = unsafe { &mut *user.cast::<Output>() };
        output.calls += 1;
        if output.calls != 1 {
            return BAD_REQUEST;
        }
        match unsafe { borrowed(bytes) } {
            Ok(bytes) => {
                output.bytes = Some(bytes.to_vec());
                OK
            }
            Err(code) => code,
        }
    }))
    .unwrap_or(ENGINE_FAILURE)
}

unsafe extern "C" fn read_block(
    user: *mut c_void,
    x: i32,
    y: i32,
    z: i32,
    out: *mut BlockReadV1,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if user.is_null() || out.is_null() {
            return BAD_REQUEST;
        }
        let bridge = unsafe { &*user.cast::<Bridge<'_>>() };
        let Some(host) = bridge.host else {
            return HOST_UNAVAILABLE;
        };
        match host.block([x, y, z]) {
            Ok(block) => {
                unsafe { out.write(block) };
                OK
            }
            Err(_) => HOST_UNAVAILABLE,
        }
    }))
    .unwrap_or(ENGINE_FAILURE)
}

unsafe extern "C" fn read_query(
    user: *mut c_void,
    operation: u32,
    request: BytesV1,
    out: *mut u8,
    capacity: u32,
    needed: *mut u32,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if user.is_null() || needed.is_null() {
            return BAD_REQUEST;
        }
        let bridge = unsafe { &*user.cast::<Bridge<'_>>() };
        let Some(host) = bridge.host else {
            return HOST_UNAVAILABLE;
        };
        let request = match unsafe { borrowed(request) } {
            Ok(v) => v,
            Err(code) => return code,
        };
        match host.query(operation, request) {
            Ok(bytes) => {
                if bytes.len() > MAX_BYTES {
                    return ENGINE_FAILURE;
                }
                unsafe { needed.write(bytes.len() as u32) };
                if (capacity as usize) < bytes.len() {
                    return BUFFER_TOO_SMALL;
                }
                if !bytes.is_empty() {
                    if out.is_null() {
                        return BAD_REQUEST;
                    }
                    // SAFETY: ABI caller supplies capacity writable bytes; this
                    // source is a separate owned response buffer.
                    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
                }
                OK
            }
            Err(error) if error == "unsupported" => UNSUPPORTED,
            Err(_) => HOST_UNAVAILABLE,
        }
    }))
    .unwrap_or(ENGINE_FAILURE)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("baritone-loader-{}-{name}", std::process::id()))
    }
    #[test]
    fn extraction_preserves_modified_library_and_override() {
        let root = root("replace");
        let path = engine_path(&root, b"stock-v1").unwrap();
        fs::write(&path, b"modified").unwrap();
        assert_eq!(engine_path(&root, b"stock-v1").unwrap(), path);
        assert_eq!(fs::read(&path).unwrap(), b"modified");
        let replacement = root.join("engine-override").join(ENGINE_FILE);
        fs::create_dir_all(replacement.parent().unwrap()).unwrap();
        fs::write(&replacement, b"custom").unwrap();
        assert_eq!(engine_path(&root, b"stock-v1").unwrap(), replacement);
        assert!(Runtime::open(&replacement).is_err());
        assert_eq!(fs::read(&replacement).unwrap(), b"custom");
    }
    #[test]
    fn rejects_relative_missing_and_wrong_architecture() {
        assert!(engine_path(Path::new("relative"), b"x").is_err());
        assert!(Runtime::open(&root("missing").join("missing.dll")).is_err());
        let root = root("arch");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("bad.dll");
        fs::write(&file, [0u8; 128]).unwrap();
        assert!(Runtime::open(&file).err().unwrap().contains("is not a"));
    }

    struct EmptyHost;
    impl Host for EmptyHost {
        fn block(&self, _: [i32; 3]) -> Result<BlockReadV1, String> {
            Ok(BlockReadV1::default())
        }
        fn query(&self, _: u32, _: &[u8]) -> Result<Vec<u8>, String> {
            Ok(b"{\"schema\":1}".to_vec())
        }
    }
    #[test]
    fn query_length_retry_and_single_emit_are_bounded() {
        let host = EmptyHost;
        let mut bridge = Bridge { host: Some(&host) };
        let mut needed = 0;
        let request = b"{\"schema\":1}";
        let bytes = BytesV1 {
            ptr: request.as_ptr(),
            len: request.len() as u32,
        };
        let user = (&mut bridge as *mut Bridge<'_>).cast();
        assert_eq!(
            unsafe { read_query(user, 7, bytes, std::ptr::null_mut(), 0, &mut needed) },
            BUFFER_TOO_SMALL
        );
        let mut output = vec![0u8; needed as usize];
        assert_eq!(
            unsafe {
                read_query(
                    user,
                    7,
                    bytes,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut needed,
                )
            },
            OK
        );
        assert_eq!(output, request);
        let mut output = Output::default();
        let user = (&mut output as *mut Output).cast();
        assert_eq!(unsafe { collect(user, bytes) }, OK);
        assert_eq!(unsafe { collect(user, bytes) }, BAD_REQUEST);
    }
    #[test]
    fn host_panics_are_contained_on_originating_side() {
        struct Panics;
        impl Host for Panics {
            fn block(&self, _: [i32; 3]) -> Result<BlockReadV1, String> {
                panic!("mock host block panic")
            }
            fn query(&self, _: u32, _: &[u8]) -> Result<Vec<u8>, String> {
                panic!("mock host query panic")
            }
        }
        let host = Panics;
        let mut bridge = Bridge { host: Some(&host) };
        let user = (&mut bridge as *mut Bridge<'_>).cast();
        let mut block = BlockReadV1::default();
        assert_eq!(
            unsafe { read_block(user, 0, 64, 0, &mut block) },
            ENGINE_FAILURE
        );
        let mut needed = 0;
        let bytes = BytesV1 {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert_eq!(
            unsafe { read_query(user, 7, bytes, std::ptr::null_mut(), 0, &mut needed) },
            ENGINE_FAILURE
        );
    }
    #[test]
    #[ignore = "requires independently built engine at SUNSET_BARITONE_TEST_DLL"]
    fn actual_dll_protocol_and_missing_host_fail_closed() {
        let path =
            PathBuf::from(std::env::var_os("SUNSET_BARITONE_TEST_DLL").expect("set test DLL path"));
        let mut runtime = Runtime::open(&path).unwrap();
        let snapshot = runtime.call(7, b"{\"schema\":1}", None).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&snapshot).unwrap()["engine"],
            "sunset-baritone/1.1.52"
        );
        assert!(runtime.call(7, b"{\"schema\":2}", None).is_err());
        assert!(runtime.call(99, b"{\"schema\":1}", None).is_err());
        let go=b"{\"schema\":1,\"world\":\"1\",\"dimension\":\"minecraft:overworld\",\"job_generation\":\"2\",\"lease_generation\":\"3\",\"goal\":[2,64,0]}";
        runtime.call(2, go, None).unwrap();
        assert!(runtime.call(5, b"{\"schema\":1}", None).is_err());
        let snapshot = runtime.call(7, b"{\"schema\":1}", None).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&snapshot).unwrap()["active"],
            false
        );
        runtime.cancel().unwrap();
    }
    #[test]
    #[ignore = "requires stock and compatible modified DLL environment paths"]
    fn actual_modified_dll_override_without_relinking_loader() {
        let original =
            PathBuf::from(std::env::var_os("SUNSET_BARITONE_TEST_DLL").expect("set stock DLL"));
        let modified = PathBuf::from(
            std::env::var_os("SUNSET_BARITONE_MODIFIED_TEST_DLL").expect("set modified DLL"),
        );
        let root = root("actual-override");
        let stock_bytes = fs::read(&original).unwrap();
        let stock = engine_path(&root, &stock_bytes).unwrap();
        let mut first = Runtime::open(&stock).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&first.call(7, b"{\"schema\":1}", None).unwrap()).unwrap();
        assert_eq!(value["build_marker"], "stock");
        drop(first);
        let override_file = root.join("engine-override").join(ENGINE_FILE);
        fs::create_dir_all(override_file.parent().unwrap()).unwrap();
        fs::copy(&modified, &override_file).unwrap();
        let path = engine_path(&root, &stock_bytes).unwrap();
        assert_eq!(path, override_file);
        let mut second = Runtime::open(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&second.call(7, b"{\"schema\":1}", None).unwrap()).unwrap();
        assert_eq!(value["build_marker"], "swarm-modified");
    }
    #[test]
    #[ignore = "requires standalone incompatible_fixture.rs DLL environment path"]
    fn actual_incompatible_abi_is_rejected_before_instance_creation() {
        let path = PathBuf::from(
            std::env::var_os("SUNSET_BARITONE_INCOMPATIBLE_TEST_DLL")
                .expect("set incompatible DLL"),
        );
        let error = Runtime::open(&path).err().expect("must reject ABI 2 table");
        assert!(error.contains("ABI/table size is incompatible"), "{error}");
    }
    #[test]
    fn required_callbacks_are_checked_without_constructing_null_function_pointers() {
        unsafe extern "C" fn create() -> *mut c_void {
            std::ptr::null_mut()
        }
        unsafe extern "C" fn destroy(_: *mut c_void) {}
        unsafe extern "C" fn dispatch(
            _: *mut c_void,
            _: u32,
            _: BytesV1,
            _: *const HostV1,
            _: EmitV1,
            _: *mut c_void,
        ) -> i32 {
            OK
        }
        let good = RawApiV1 {
            abi: 1,
            size: size_of::<ApiV1>() as u32,
            create: Some(create),
            destroy: Some(destroy),
            dispatch: Some(dispatch),
        };
        assert!(good.checked().is_ok());
        for missing in [
            RawApiV1 {
                create: None,
                ..good
            },
            RawApiV1 {
                destroy: None,
                ..good
            },
            RawApiV1 {
                dispatch: None,
                ..good
            },
        ] {
            assert!(
                missing
                    .checked()
                    .err()
                    .unwrap()
                    .contains("missing a required callback")
            );
        }
    }
    #[test]
    #[ignore = "requires ABI-compatible header / null callback test DLL path"]
    fn actual_null_callback_table_is_rejected_before_instance_creation() {
        let path = PathBuf::from(
            std::env::var_os("SUNSET_BARITONE_NULL_CALLBACK_TEST_DLL")
                .expect("set null-callback DLL"),
        );
        let error = Runtime::open(&path)
            .err()
            .expect("must reject null callback");
        assert!(error.contains("missing a required callback"), "{error}");
    }
}
