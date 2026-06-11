use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use log::{info, error};
use crate::{BinaryDiffEngine, DiffResult};

fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            error!("Panic caught in FFI boundary");
            default
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_init() -> *mut BinaryDiffEngine {
    guard(std::ptr::null_mut(), || {
        let _ = env_logger::try_init();
        info!("Initializing Rust Diff engine");
        Box::into_raw(Box::new(BinaryDiffEngine::new()))
    })
}

#[no_mangle]
pub extern "C" fn rust_diff_cleanup(engine: *mut BinaryDiffEngine) {
    guard((), || {
        if !engine.is_null() {
            unsafe {
                let _ = Box::from_raw(engine);
            }
        }
    })
}

/// Diff two binaries from JSON-encoded function arrays and return the full
/// DiffResult as a JSON C string (caller frees with rust_diff_free_string).
/// Returns null on parse/diff failure. This is the entry point the Binary
/// Ninja Python frontend uses; no engine handle is needed.
#[no_mangle]
pub extern "C" fn rust_diff_diff_json(
    functions_a_json: *const c_char,
    functions_b_json: *const c_char,
) -> *mut c_char {
    guard(std::ptr::null_mut(), || {
        if functions_a_json.is_null() || functions_b_json.is_null() {
            return std::ptr::null_mut();
        }

        let json_a = match unsafe { CStr::from_ptr(functions_a_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let json_b = match unsafe { CStr::from_ptr(functions_b_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };

        let _ = env_logger::try_init();
        let engine = BinaryDiffEngine::new();
        let result = match engine.perform_diff_json(json_a, json_b) {
            Ok(r) => r,
            Err(e) => {
                error!("JSON diff failed: {}", e);
                return std::ptr::null_mut();
            }
        };

        match serde_json::to_string(&result) {
            Ok(s) => match CString::new(s) {
                Ok(cs) => cs.into_raw(),
                Err(_) => std::ptr::null_mut(),
            },
            Err(e) => {
                error!("Failed to serialize diff result: {}", e);
                std::ptr::null_mut()
            }
        }
    })
}

/// Semantic IL diff between two functions. `request_json` is an
/// `il::IlDiffRequest` (two IL token streams plus an optional callee rename
/// map); returns an `il::IlDiff` as a JSON C string (caller frees with
/// rust_diff_free_string), or null on parse failure. Unlike a textual diff,
/// this normalizes volatile tokens and resolves matched-callee renames, so a
/// renamed call is reported as cosmetic while a replaced call is a real change.
/// Callable directly from the Python frontend at diff-view time.
#[no_mangle]
pub extern "C" fn rust_diff_il_diff_json(request_json: *const c_char) -> *mut c_char {
    guard(std::ptr::null_mut(), || {
        if request_json.is_null() {
            return std::ptr::null_mut();
        }
        let json = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: crate::il::IlDiffRequest = match serde_json::from_str(json) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse IL diff request: {}", e);
                return std::ptr::null_mut();
            }
        };
        let diff = crate::il::diff_il(&request);
        match serde_json::to_string(&diff) {
            Ok(s) => match CString::new(s) {
                Ok(cs) => cs.into_raw(),
                Err(_) => std::ptr::null_mut(),
            },
            Err(e) => {
                error!("Failed to serialize IL diff: {}", e);
                std::ptr::null_mut()
            }
        }
    })
}

/// Basic-block correspondence between two matched functions. `request_json`
/// is a `block::BlockDiffRequest` (two functions as IL basic blocks with CFG
/// successors, plus the optional rename/addr maps); returns a
/// `block::BlockDiff` as a JSON C string (caller frees with
/// rust_diff_free_string), or null on parse failure:
/// `{ pairs: [{a, b, status, similarity}], only_a: [...], only_b: [...], similarity }`.
/// Drives the graph diff overlay: per-pair status is equal / cosmetic / changed.
#[no_mangle]
pub extern "C" fn rust_diff_block_diff_json(request_json: *const c_char) -> *mut c_char {
    guard(std::ptr::null_mut(), || {
        if request_json.is_null() {
            return std::ptr::null_mut();
        }
        let json = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let request: crate::block::BlockDiffRequest = match serde_json::from_str(json) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse block diff request: {}", e);
                return std::ptr::null_mut();
            }
        };
        let diff = crate::block::block_diff(&request);
        match serde_json::to_string(&diff) {
            Ok(s) => match CString::new(s) {
                Ok(cs) => cs.into_raw(),
                Err(_) => std::ptr::null_mut(),
            },
            Err(e) => {
                error!("Failed to serialize block diff: {}", e);
                std::ptr::null_mut()
            }
        }
    })
}

/// Free a string returned by rust_diff_diff_json.
#[no_mangle]
pub extern "C" fn rust_diff_free_string(s: *mut c_char) {
    guard((), || {
        if !s.is_null() {
            unsafe {
                let _ = CString::from_raw(s);
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn rust_diff_free_result(result: *mut DiffResult) {
    guard((), || {
        if !result.is_null() {
            unsafe {
                let _ = Box::from_raw(result);
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn rust_diff_get_match_count(result: *const DiffResult) -> usize {
    guard(0, || {
        if result.is_null() {
            return 0;
        }
        let result = unsafe { &*result };
        result.matched_functions.len()
    })
}

#[no_mangle]
pub extern "C" fn rust_diff_get_similarity_score(result: *const DiffResult) -> f64 {
    guard(0.0, || {
        if result.is_null() {
            return 0.0;
        }
        let result = unsafe { &*result };
        result.similarity_score
    })
}

#[no_mangle]
pub extern "C" fn rust_diff_save_results(
    result: *const DiffResult,
    output_path: *const c_char,
) -> i32 {
    guard(-1, || {
        if result.is_null() || output_path.is_null() {
            return -1;
        }

        let result = unsafe { &*result };
        let output_path = unsafe { CStr::from_ptr(output_path) };
        let output_path = match output_path.to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        let engine = BinaryDiffEngine::new();
        match engine.save_results(result, output_path) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    })
}
