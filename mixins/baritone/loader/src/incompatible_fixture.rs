// Standalone negative-test DLL, never a module of the production loader.
// Build with rustc --crate-type cdylib --edition 2024 -Dwarnings, output on D:.
pub mod abi;
use abi::{ApiV1, BytesV1, EmitV1, HostV1};
use core::ffi::c_void;

#[repr(C)]
struct NullableApi {
    abi: u32,
    size: u32,
    create: Option<unsafe extern "C" fn() -> *mut c_void>,
    destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    dispatch: Option<
        unsafe extern "C" fn(*mut c_void, u32, BytesV1, *const HostV1, EmitV1, *mut c_void) -> i32,
    >,
}

unsafe extern "C" fn create() -> *mut c_void {
    std::ptr::dangling_mut::<u8>().cast()
}
unsafe extern "C" fn destroy(_engine: *mut c_void) {}
unsafe extern "C" fn dispatch(
    _engine: *mut c_void,
    _operation: u32,
    _request: BytesV1,
    _host: *const HostV1,
    _emit: EmitV1,
    _user: *mut c_void,
) -> i32 {
    abi::UNSUPPORTED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_baritone_api_v1(out: *mut ApiV1, capacity: u32) -> i32 {
    if out.is_null() {
        return abi::BAD_REQUEST;
    }
    if capacity < std::mem::size_of::<ApiV1>() as u32 {
        return abi::BUFFER_TOO_SMALL;
    }
    if option_env!("SUNSET_BARITONE_FIXTURE_NULL_CALLBACK").is_some() {
        let table = NullableApi {
            abi: 1,
            size: std::mem::size_of::<ApiV1>() as u32,
            create: None,
            destroy: Some(destroy),
            dispatch: Some(dispatch),
        };
        // A deliberately malformed nullable table is written as bytes of its
        // nullable representation, never constructed as an invalid ApiV1.
        unsafe { out.cast::<NullableApi>().write(table) };
    } else {
        let table = ApiV1 {
            abi: 2,
            size: std::mem::size_of::<ApiV1>() as u32,
            create,
            destroy,
            dispatch,
        };
        unsafe { out.write(table) };
    }
    abi::OK
}
