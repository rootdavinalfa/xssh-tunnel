mod client;

use std::os::raw::c_void;
use std::sync::OnceLock;

use serde::Serialize;

pub use client::HelperClient;

#[derive(Debug, Clone, Serialize)]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
}

struct SmAppFunctions {
    register: unsafe fn(*const c_void, *const c_void) -> bool,
    unregister: unsafe fn(*const c_void),
    copy_status: unsafe fn(*const c_void) -> *const c_void,
}

static SMAPP_FUNCTIONS: OnceLock<Result<SmAppFunctions, String>> = OnceLock::new();

fn get_smapp_fns() -> Result<&'static SmAppFunctions, String> {
    match SMAPP_FUNCTIONS.get_or_init(|| load_smapp_fns()) {
        Ok(fns) => Ok(fns),
        Err(e) => Err(e.clone()),
    }
}

fn load_smapp_fns() -> Result<SmAppFunctions, String> {
    #[cfg(not(target_os = "macos"))]
    {
        return Err("SMAppService is only available on macOS".to_string());
    }

    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::CStr;
        let handle = libc::dlopen(
            CStr::from_bytes_with_nul(b"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement\0")
                .unwrap()
                .as_ptr(),
            libc::RTLD_LAZY,
        );
        if handle.is_null() {
            return Err("ServiceManagement framework not available".to_string());
        }

        let register = libc::dlsym(handle, CStr::from_bytes_with_nul(b"SMAppServiceRegister\0").unwrap().as_ptr());
        let unregister = libc::dlsym(handle, CStr::from_bytes_with_nul(b"SMAppServiceUnregister\0").unwrap().as_ptr());
        let copy_status = libc::dlsym(handle, CStr::from_bytes_with_nul(b"SMAppServiceCopyStatus\0").unwrap().as_ptr());

        if register.is_null() || unregister.is_null() || copy_status.is_null() {
            return Err("SMAppService functions not found (requires macOS 13+)".to_string());
        }

        Ok(SmAppFunctions {
            register: std::mem::transmute(register),
            unregister: std::mem::transmute(unregister),
            copy_status: std::mem::transmute(copy_status),
        })
    }
}

#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "Security", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(alloc: *const c_void, c_str: *const std::os::raw::c_char, encoding: u32) -> *const c_void;
    fn CFRelease(cf: *const c_void);
    fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    fn CFStringCompare(str1: *const c_void, str2: *const c_void, options: u32) -> i32;

    fn AuthorizationCreate(
        rights: *const c_void,
        environment: *const c_void,
        flags: u32,
        authorization: *mut *const c_void,
    ) -> i32;
}

const K_CFSTRING_ENCODING_UTF8: u32 = 0x08000100;
const K_CFCOMPARE_EQUAL_TO: i32 = 0;

pub fn get_status() -> Result<HelperStatus, String> {
    let fns = get_smapp_fns()?;
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");

    unsafe {
        let status = (fns.copy_status)(service_name);
        CFRelease(service_name);

        let installed = !status.is_null();
        let mut running = false;

        if installed {
            let status_key = make_cfstring("Status");
            let enabled_key = make_cfstring("enabled");

            let status_val = CFDictionaryGetValue(status, status_key);
            if !status_val.is_null() {
                if CFStringCompare(status_val, enabled_key, 0) == K_CFCOMPARE_EQUAL_TO {
                    running = true;
                }
            }

            CFRelease(status_key);
            CFRelease(enabled_key);
            CFRelease(status);
        }

        Ok(HelperStatus { installed, running })
    }
}

pub fn install(bundle_path: &str) -> Result<(), String> {
    let fns = get_smapp_fns()?;
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");
    let path_cf = make_cfstring(bundle_path);

    unsafe {
        let mut auth: *const c_void = std::ptr::null();
        let status = AuthorizationCreate(
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut auth,
        );
        if status != 0 || auth.is_null() {
            CFRelease(service_name);
            CFRelease(path_cf);
            return Err("Failed to get admin authorization".to_string());
        }

        let success = (fns.register)(service_name, path_cf);

        CFRelease(auth);
        CFRelease(service_name);
        CFRelease(path_cf);

        if !success {
            return Err("SMAppServiceRegister failed".to_string());
        }
    }

    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let fns = get_smapp_fns()?;
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");

    unsafe {
        (fns.unregister)(service_name);
        CFRelease(service_name);
    }

    Ok(())
}

fn make_cfstring(s: &str) -> *const c_void {
    use std::ffi::CString;
    let c_str = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c_str.as_ptr(), K_CFSTRING_ENCODING_UTF8) }
}
