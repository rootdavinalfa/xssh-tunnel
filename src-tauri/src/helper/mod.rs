mod client;

use std::os::raw::c_void;

use serde::Serialize;

pub use client::HelperClient;

#[derive(Debug, Clone, Serialize)]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
}

#[link(name = "ServiceManagement", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SMAppServiceRegister(service_name: *const c_void, bundle_path: *const c_void) -> bool;
    fn SMAppServiceUnregister(service_name: *const c_void);
    fn SMAppServiceCopyStatus(service_name: *const c_void) -> *const c_void;

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

const kCFStringEncodingUTF8: u32 = 0x08000100;
const kCFCompareEqualTo: i32 = 0;

pub fn get_status() -> Result<HelperStatus, String> {
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");

    unsafe {
        let status = SMAppServiceCopyStatus(service_name);
        CFRelease(service_name);

        let installed = !status.is_null();
        let mut running = false;

        if installed {
            let status_key = make_cfstring("Status");
            let enabled_key = make_cfstring("enabled");

            let status_val = CFDictionaryGetValue(status, status_key);
            if !status_val.is_null() {
                if CFStringCompare(status_val, enabled_key, 0) == kCFCompareEqualTo {
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
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");
    let path_cf = make_cfstring(bundle_path);

    unsafe {
        // First create authorization to prompt for admin credentials
        let mut auth: *const c_void = std::ptr::null();
        let status = AuthorizationCreate(
            std::ptr::null(),
            std::ptr::null(),
            0, // kAuthorizationFlagDefaults
            &mut auth,
        );
        if status != 0 || auth.is_null() {
            CFRelease(service_name);
            CFRelease(path_cf);
            return Err("Failed to get admin authorization".to_string());
        }

        let success = SMAppServiceRegister(service_name, path_cf);

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
    let service_name = make_cfstring("xyz.dvnlabs.xsshtunnel");

    unsafe {
        SMAppServiceUnregister(service_name);
        CFRelease(service_name);
    }

    Ok(())
}

fn make_cfstring(s: &str) -> *const c_void {
    use std::ffi::CString;
    let c_str = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c_str.as_ptr(), kCFStringEncodingUTF8) }
}