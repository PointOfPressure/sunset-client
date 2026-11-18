// Independently authored C interface. Distributed under mixins/baritone/LICENSE (MIT).
use core::ffi::c_void;

pub const ABI_V1: u32 = 1;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TARGETS: usize = 4096;
pub const MAX_PATH: usize = 2048;
pub const OK: i32 = 0;
pub const UNSUPPORTED: i32 = 1;
pub const BAD_REQUEST: i32 = 2;
pub const HOST_UNAVAILABLE: i32 = 3;
pub const BUFFER_TOO_SMALL: i32 = 4;
pub const ENGINE_FAILURE: i32 = 5;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct BytesV1 {
    pub ptr: *const u8,
    pub len: u32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct BlockReadV1 {
    pub state_id: u32,
    pub server_state_id: u32,
    pub loaded: u32,
    pub prediction_pending: u32,
    pub prediction_sequence: i32,
}

pub type EmitV1 = unsafe extern "C" fn(user: *mut c_void, bytes: BytesV1) -> i32;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct HostV1 {
    pub abi: u32,
    pub size: u32,
    pub user: *mut c_void,
    pub block: unsafe extern "C" fn(
        user: *mut c_void,
        x: i32,
        y: i32,
        z: i32,
        out: *mut BlockReadV1,
    ) -> i32,
    pub query: unsafe extern "C" fn(
        user: *mut c_void,
        operation: u32,
        request: BytesV1,
        out: *mut u8,
        capacity: u32,
        needed: *mut u32,
    ) -> i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ApiV1 {
    pub abi: u32,
    pub size: u32,
    pub create: unsafe extern "C" fn() -> *mut c_void,
    pub destroy: unsafe extern "C" fn(engine: *mut c_void),
    pub dispatch: unsafe extern "C" fn(
        engine: *mut c_void,
        operation: u32,
        request: BytesV1,
        host: *const HostV1,
        emit: EmitV1,
        emit_user: *mut c_void,
    ) -> i32,
}

// Exported discovery symbol: horizon_baritone_api_v1.
pub type DiscoverV1 = unsafe extern "C" fn(out: *mut ApiV1, capacity: u32) -> i32;
