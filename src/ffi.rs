use std::ffi::CStr;
use std::os::raw::c_char;
use log::{info, error};
use crate::{BinaryDiffEngine, DiffResult};

#[no_mangle]
pub extern "C" fn rust_diff_init() -> *mut BinaryDiffEngine {
    let _ = env_logger::try_init();
    info!("Initializing Rust Diff engine");

    let engine = Box::new(BinaryDiffEngine::new());
    Box::into_raw(engine)
}

#[no_mangle]
pub extern "C" fn rust_diff_cleanup(engine: *mut BinaryDiffEngine) {
    if !engine.is_null() {
        unsafe {
            let _ = Box::from_raw(engine);
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_perform_diff_mock(
    engine: *mut BinaryDiffEngine,
    binary_a_name: *const c_char,
    binary_b_name: *const c_char,
) -> *mut DiffResult {
    if engine.is_null() || binary_a_name.is_null() || binary_b_name.is_null() {
        return std::ptr::null_mut();
    }

    let engine = unsafe { &mut *engine };
    let binary_a_name = unsafe { CStr::from_ptr(binary_a_name) };
    let binary_b_name = unsafe { CStr::from_ptr(binary_b_name) };

    let binary_a_name = match binary_a_name.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let binary_b_name = match binary_b_name.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match engine.perform_diff_mock(binary_a_name, binary_b_name) {
        Ok(result) => Box::into_raw(Box::new(result)),
        Err(e) => {
            error!("Diff failed: {}", e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_free_result(result: *mut DiffResult) {
    if !result.is_null() {
        unsafe {
            let _ = Box::from_raw(result);
        }
    }
}

#[no_mangle]
pub extern "C" fn rust_diff_get_match_count(result: *const DiffResult) -> usize {
    if result.is_null() {
        return 0;
    }

    let result = unsafe { &*result };
    result.matched_functions.len()
}

#[no_mangle]
pub extern "C" fn rust_diff_get_similarity_score(result: *const DiffResult) -> f64 {
    if result.is_null() {
        return 0.0;
    }

    let result = unsafe { &*result };
    result.similarity_score
}

#[no_mangle]
pub extern "C" fn rust_diff_save_results(
    result: *const DiffResult,
    output_path: *const c_char,
) -> i32 {
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
}
