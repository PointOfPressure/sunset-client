// Engine end of the MIT transport. Compiled by the independent engine through
// an explicit #[path], so every raw-pointer implementation stays in loader/src.
// This file is not linked into the game-side loader. No engine implementation
// is a Cargo dependency of the loader.
use crate::abi::*;
use crate::engine::Engine;
use crate::host::Host;
use crate::protocol;
use core::ffi::c_void;
use serde_json::Value;
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::{self, ThreadId};

struct Handle {
    engine: RefCell<Engine>,
    creator: ThreadId,
}
struct ForeignHost {
    table: HostV1,
}
#[repr(C)]
struct RawHostV1 {
    abi: u32,
    size: u32,
    user: *mut c_void,
    block: Option<unsafe extern "C" fn(*mut c_void, i32, i32, i32, *mut BlockReadV1) -> i32>,
    query: Option<unsafe extern "C" fn(*mut c_void, u32, BytesV1, *mut u8, u32, *mut u32) -> i32>,
}
const _: () = {
    assert!(size_of::<RawHostV1>() == size_of::<HostV1>());
    assert!(align_of::<RawHostV1>() == align_of::<HostV1>());
};

impl Host for ForeignHost {
    fn block(&self, pos: [i32; 3]) -> protocol::Result<BlockReadV1> {
        let mut output = BlockReadV1::default();
        let code =
            unsafe { (self.table.block)(self.table.user, pos[0], pos[1], pos[2], &mut output) };
        if code == OK { Ok(output) } else { Err(code) }
    }
    fn query(&self, operation: u32, request: &Value) -> protocol::Result<Value> {
        let request = serde_json::to_vec(request).map_err(|_| BAD_REQUEST)?;
        let bytes = BytesV1 {
            ptr: request.as_ptr(),
            len: request.len() as u32,
        };
        let mut needed = 0;
        let code = unsafe {
            (self.table.query)(
                self.table.user,
                operation,
                bytes,
                std::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        if code != OK && code != BUFFER_TOO_SMALL {
            return Err(code);
        }
        if needed as usize > MAX_BYTES {
            return Err(HOST_UNAVAILABLE);
        }
        let mut output = vec![0u8; needed as usize];
        let capacity = needed;
        let code = unsafe {
            (self.table.query)(
                self.table.user,
                operation,
                bytes,
                output.as_mut_ptr(),
                capacity,
                &mut needed,
            )
        };
        if code != OK {
            return Err(code);
        }
        if needed > capacity {
            return Err(HOST_UNAVAILABLE);
        }
        output.truncate(needed as usize);
        protocol::parse(&output).map_err(|_| HOST_UNAVAILABLE)
    }
}

unsafe fn bytes<'a>(request: BytesV1) -> protocol::Result<&'a [u8]> {
    if request.len as usize > MAX_BYTES || (request.len > 0 && request.ptr.is_null()) {
        return Err(BAD_REQUEST);
    }
    if request.len == 0 {
        return Ok(&[]);
    }
    // SAFETY: nonnull request is a borrowed readable buffer by the ABI contract;
    // the returned slice is consumed before dispatch returns and never retained.
    Ok(unsafe { std::slice::from_raw_parts(request.ptr, request.len as usize) })
}

unsafe extern "C" fn create() -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        Box::into_raw(Box::new(Handle {
            engine: RefCell::new(Engine::new()),
            creator: thread::current().id(),
        }))
        .cast()
    }))
    .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn destroy(engine: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || !(engine as usize).is_multiple_of(align_of::<Handle>()) {
            return;
        }
        // SAFETY: destroy accepts exactly one live handle created by this DLL.
        // Reentrant/cross-thread destruction fails closed, retaining the handle.
        let handle = unsafe { &*engine.cast::<Handle>() };
        if handle.creator != thread::current().id() || handle.engine.try_borrow_mut().is_err() {
            return;
        }
        unsafe { drop(Box::from_raw(engine.cast::<Handle>())) };
    }));
}

unsafe extern "C" fn dispatch(
    engine: *mut c_void,
    operation: u32,
    request: BytesV1,
    host: *const HostV1,
    emit: EmitV1,
    emit_user: *mut c_void,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() || !(engine as usize).is_multiple_of(align_of::<Handle>()) {
            return BAD_REQUEST;
        }
        let handle = unsafe { &*engine.cast::<Handle>() };
        if handle.creator != thread::current().id() {
            return ENGINE_FAILURE;
        }
        let Ok(mut engine) = handle.engine.try_borrow_mut() else {
            return ENGINE_FAILURE;
        };
        let result = (|| {
            let value = protocol::parse(unsafe { bytes(request) }?)?;
            let host = if host.is_null() {
                None
            } else {
                if !(host as usize).is_multiple_of(align_of::<HostV1>()) {
                    return Err(BAD_REQUEST);
                }
                // Check only the POD header before reading callback fields:
                // an incompatible host may supply just those two words.
                let header = host.cast::<u32>();
                let (abi, size) = unsafe { (header.read(), header.add(1).read()) };
                if abi != ABI_V1 || size != size_of::<HostV1>() as u32 {
                    return Err(UNSUPPORTED);
                }
                let raw = unsafe { host.cast::<RawHostV1>().read() };
                let (Some(block), Some(query)) = (raw.block, raw.query) else {
                    return Err(HOST_UNAVAILABLE);
                };
                let table = HostV1 {
                    abi: raw.abi,
                    size: raw.size,
                    user: raw.user,
                    block,
                    query,
                };
                Some(ForeignHost { table })
            };
            let reply =
                engine.dispatch(operation, &value, host.as_ref().map(|h| h as &dyn Host))?;
            let output = serde_json::to_vec(&reply).map_err(|_| ENGINE_FAILURE)?;
            if output.len() > MAX_BYTES {
                return Err(ENGINE_FAILURE);
            }
            let code = unsafe {
                emit(
                    emit_user,
                    BytesV1 {
                        ptr: output.as_ptr(),
                        len: output.len() as u32,
                    },
                )
            };
            if code == OK {
                Ok(())
            } else {
                Err(ENGINE_FAILURE)
            }
        })();
        match result {
            Ok(()) => OK,
            Err(code) => {
                engine.cancel("C ABI request failed");
                code
            }
        }
    }));
    match result {
        Ok(code) => code,
        Err(_) => {
            // Recoverable Rust panic never unwinds into the host. Drop all pending
            // actions before the next call can obtain a response from this engine.
            if !engine.is_null() && (engine as usize).is_multiple_of(align_of::<Handle>()) {
                let handle = unsafe { &*engine.cast::<Handle>() };
                if handle.creator == thread::current().id() {
                    if let Ok(mut engine) = handle.engine.try_borrow_mut() {
                        engine.cancel("engine panic");
                    }
                }
            }
            ENGINE_FAILURE
        }
    }
}

/// Discover the versioned engine table. Caller owns uninitialized writable
/// ApiV1 storage; success initializes every byte containing a field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn horizon_baritone_api_v1(out: *mut ApiV1, capacity: u32) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if out.is_null() || !(out as usize).is_multiple_of(align_of::<ApiV1>()) {
            return BAD_REQUEST;
        }
        if capacity < size_of::<ApiV1>() as u32 {
            return BUFFER_TOO_SMALL;
        }
        let api = ApiV1 {
            abi: ABI_V1,
            size: size_of::<ApiV1>() as u32,
            create,
            destroy,
            dispatch,
        };
        // SAFETY: capacity/alignment checked and caller supplies writable storage.
        unsafe { out.write(api) };
        OK
    }))
    .unwrap_or(ENGINE_FAILURE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::MaybeUninit;
    unsafe extern "C" fn collect(user: *mut c_void, bytes: BytesV1) -> i32 {
        let output = unsafe { &mut *user.cast::<Vec<u8>>() };
        *output = unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len as usize) }.to_vec();
        OK
    }
    unsafe extern "C" fn reject(_user: *mut c_void, _bytes: BytesV1) -> i32 {
        BAD_REQUEST
    }
    fn api() -> ApiV1 {
        let mut value = MaybeUninit::uninit();
        assert_eq!(
            unsafe { horizon_baritone_api_v1(value.as_mut_ptr(), size_of::<ApiV1>() as u32) },
            OK
        );
        unsafe { value.assume_init() }
    }
    fn call(api: &ApiV1, engine: *mut c_void, op: u32, request: &[u8]) -> (i32, Vec<u8>) {
        let mut output = Vec::new();
        let code = unsafe {
            (api.dispatch)(
                engine,
                op,
                BytesV1 {
                    ptr: request.as_ptr(),
                    len: request.len() as u32,
                },
                std::ptr::null(),
                collect,
                (&mut output as *mut Vec<u8>).cast(),
            )
        };
        (code, output)
    }
    #[test]
    fn abi_discovery_sizes_and_null_output_fail_without_initializing_table() {
        let mut value = MaybeUninit::<ApiV1>::uninit();
        assert_eq!(
            unsafe { horizon_baritone_api_v1(value.as_mut_ptr(), 0) },
            BUFFER_TOO_SMALL
        );
        assert_eq!(
            unsafe { horizon_baritone_api_v1(std::ptr::null_mut(), size_of::<ApiV1>() as u32) },
            BAD_REQUEST
        );
        let table = api();
        assert_eq!(table.abi, 1);
        assert_eq!(table.size, size_of::<ApiV1>() as u32);
    }
    #[test]
    fn abi_create_snapshot_cancel_and_destroy_without_host() {
        let api = api();
        let engine = unsafe { (api.create)() };
        assert!(!engine.is_null());
        let (code, bytes) = call(&api, engine, 7, b"{\"schema\":1}");
        assert_eq!(code, OK);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["capabilities"].as_array().unwrap().len(), 3);
        assert_eq!(call(&api, engine, 6, b"{\"schema\":1}").0, OK);
        unsafe { (api.destroy)(engine) };
    }
    #[test]
    fn abi_bad_json_oversize_unknown_operations_and_missing_host_fail_closed() {
        let api = api();
        let engine = unsafe { (api.create)() };
        for (op, request, expected) in [
            (7, b"not-json".as_slice(), BAD_REQUEST),
            (99, b"{\"schema\":1}".as_slice(), UNSUPPORTED),
            (5, b"{\"schema\":1}".as_slice(), HOST_UNAVAILABLE),
        ] {
            let (code, bytes) = call(&api, engine, op, request);
            assert_eq!(code, expected);
            assert!(bytes.is_empty());
        }
        let code = unsafe {
            (api.dispatch)(
                engine,
                7,
                BytesV1 {
                    ptr: std::ptr::null(),
                    len: MAX_BYTES as u32 + 1,
                },
                std::ptr::null(),
                reject,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, BAD_REQUEST);
        let (_, snapshot) = call(&api, engine, 7, b"{\"schema\":1}");
        assert_eq!(
            serde_json::from_slice::<Value>(&snapshot).unwrap()["active"],
            false
        );
        unsafe { (api.destroy)(engine) };
    }
    #[test]
    fn abi_emit_rejection_does_not_reexecute_a_mutating_operation() {
        let api = api();
        let engine = unsafe { (api.create)() };
        let request=b"{\"schema\":1,\"world\":\"1\",\"dimension\":\"minecraft:overworld\",\"job_generation\":\"2\",\"lease_generation\":\"3\",\"goal\":[2,64,0]}";
        let code = unsafe {
            (api.dispatch)(
                engine,
                2,
                BytesV1 {
                    ptr: request.as_ptr(),
                    len: request.len() as u32,
                },
                std::ptr::null(),
                reject,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, ENGINE_FAILURE);
        let (_, snapshot) = call(&api, engine, 7, b"{\"schema\":1}");
        assert_eq!(
            serde_json::from_slice::<Value>(&snapshot).unwrap()["active"],
            false
        );
        unsafe { (api.destroy)(engine) };
    }
    #[test]
    fn abi_reentrant_dispatch_is_rejected_without_aliasing_mutable_engine() {
        struct Reentrant {
            api: ApiV1,
            engine: *mut c_void,
            code: i32,
        }
        unsafe extern "C" fn reenter(user: *mut c_void, _bytes: BytesV1) -> i32 {
            let data = unsafe { &mut *user.cast::<Reentrant>() };
            data.code = call(&data.api, data.engine, 7, b"{\"schema\":1}").0;
            OK
        }
        let api = api();
        let engine = unsafe { (api.create)() };
        let mut data = Reentrant {
            api,
            engine,
            code: 0,
        };
        let request = b"{\"schema\":1}";
        let code = unsafe {
            (api.dispatch)(
                engine,
                7,
                BytesV1 {
                    ptr: request.as_ptr(),
                    len: request.len() as u32,
                },
                std::ptr::null(),
                reenter,
                (&mut data as *mut Reentrant).cast(),
            )
        };
        assert_eq!(code, OK);
        assert_eq!(data.code, ENGINE_FAILURE);
        unsafe { (api.destroy)(engine) };
    }

    #[test]
    fn abi_header_only_host_is_rejected_before_reading_callback_fields() {
        #[repr(C, align(8))]
        struct Header {
            abi: u32,
            size: u32,
        }
        let api = api();
        let engine = unsafe { (api.create)() };
        let request = b"{\"schema\":1}";
        for header in [Header { abi: 1, size: 8 }, Header { abi: 2, size: 32 }] {
            let mut output = Vec::<u8>::new();
            let code = unsafe {
                (api.dispatch)(
                    engine,
                    7,
                    BytesV1 {
                        ptr: request.as_ptr(),
                        len: request.len() as u32,
                    },
                    (&header as *const Header).cast(),
                    collect,
                    (&mut output as *mut Vec<u8>).cast(),
                )
            };
            assert_eq!(code, UNSUPPORTED);
            assert!(output.is_empty());
        }
        unsafe { (api.destroy)(engine) };
    }
    #[test]
    fn abi_null_host_callbacks_fail_closed_without_invalid_fn_values() {
        let api = api();
        let engine = unsafe { (api.create)() };
        let host = RawHostV1 {
            abi: 1,
            size: size_of::<HostV1>() as u32,
            user: std::ptr::null_mut(),
            block: None,
            query: None,
        };
        let request = b"{\"schema\":1}";
        let mut output = Vec::<u8>::new();
        let code = unsafe {
            (api.dispatch)(
                engine,
                7,
                BytesV1 {
                    ptr: request.as_ptr(),
                    len: request.len() as u32,
                },
                (&host as *const RawHostV1).cast(),
                collect,
                (&mut output as *mut Vec<u8>).cast(),
            )
        };
        assert_eq!(code, HOST_UNAVAILABLE);
        assert!(output.is_empty());
        unsafe { (api.destroy)(engine) };
    }
}
