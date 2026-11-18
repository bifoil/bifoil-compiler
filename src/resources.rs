//! Native process allocation policy and platform memory enforcement.
//!
//! Reader threads must not reserve separate large glibc arenas inside the same
//! address-space budget used for actual compiler data.

pub(crate) fn initialize() -> Result<(), String> {
    allocator_policy()?;
    memory_limit()
}

fn checked_system_call(success: bool, message: &str) -> Result<(), String> {
    if success { Ok(()) } else { Err(message.to_owned()) }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn allocator_policy() -> Result<(), String> {
    use std::ffi::c_int;
    // glibc's <malloc.h>: keep the main arena and share it with I/O workers.
    const M_ARENA_MAX: c_int = -8;
    unsafe extern "C" {
        fn mallopt(parameter: c_int, value: c_int) -> c_int;
    }
    // No worker exists yet. This changes only this process's allocation policy
    // and leaves the stricter existing address-space limit intact.
    checked_system_call(
        unsafe { mallopt(M_ARENA_MAX, 1) } != 0,
        "cannot bound compiler allocator arenas",
    )
}
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn allocator_policy() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn memory_limit() -> Result<(), String> {
    #[repr(C)]
    struct RLimit {
        current: u64,
        maximum: u64,
    }
    unsafe extern "C" {
        fn setrlimit(resource: i32, limit: *const RLimit) -> i32;
        fn getrlimit(resource: i32, limit: *mut RLimit) -> i32;
    }
    let mut limit = RLimit { current: 0, maximum: 0 };
    unsafe {
        checked_system_call(getrlimit(9, &mut limit) == 0, "cannot inspect compiler memory limit")?;
        limit.current = limit.current.min(240_000_000);
        checked_system_call(setrlimit(9, &limit) == 0, "cannot enforce compiler memory limit")?;
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn memory_limit() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::checked_system_call;

    #[test]
    fn system_call_failures_keep_the_operation_specific_diagnostic() {
        for message in [
            "cannot bound compiler allocator arenas",
            "cannot inspect compiler memory limit",
            "cannot enforce compiler memory limit",
        ] {
            assert_eq!(checked_system_call(true, message), Ok(()));
            assert_eq!(checked_system_call(false, message), Err(message.to_owned()));
        }
    }
}
