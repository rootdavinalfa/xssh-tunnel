mod client;

use serde::Serialize;

pub use client::HelperClient;

#[derive(Debug, Clone, Serialize)]
pub struct HelperStatus {
    pub installed: bool,
    pub running: bool,
}

#[cfg(target_os = "macos")]
type AnyObject = objc2::runtime::AnyObject;

/// Install the privileged helper via SMAppService Objective-C API.
/// bundle_path is the path to the helper executable inside the app bundle.
pub fn install(_bundle_path: &str) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    { return Err("SMAppService is only available on macOS".to_string()); }

    #[cfg(target_os = "macos")]
    {
        let cls = objc2::class!(SMAppService);
        let service: *mut AnyObject = unsafe { objc2::msg_send![cls, mainAppService] };

        let mut error: *mut AnyObject = std::ptr::null_mut();
        let success: bool = unsafe { objc2::msg_send![service, registerAndReturnError: &mut error] };

        if !success {
            let desc: *mut AnyObject = unsafe { objc2::msg_send![error, localizedDescription] };
            let c_str: *const std::os::raw::c_char = unsafe { objc2::msg_send![desc, UTF8String] };
            let err_msg = unsafe { std::ffi::CStr::from_ptr(c_str) }.to_string_lossy().to_string();
            return Err(format!("SMAppService install failed: {}", err_msg));
        }

        Ok(())
    }
}

/// Uninstall the privileged helper
pub fn uninstall() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    { return Err("SMAppService is only available on macOS".to_string()); }

    #[cfg(target_os = "macos")]
    {
        let cls = objc2::class!(SMAppService);
        let service: *mut AnyObject = unsafe { objc2::msg_send![cls, mainAppService] };

        let mut error: *mut AnyObject = std::ptr::null_mut();
        let success: bool = unsafe { objc2::msg_send![service, unregisterAndReturnError: &mut error] };

        if !success {
            let desc: *mut AnyObject = unsafe { objc2::msg_send![error, localizedDescription] };
            let c_str: *const std::os::raw::c_char = unsafe { objc2::msg_send![desc, UTF8String] };
            let err_msg = unsafe { std::ffi::CStr::from_ptr(c_str) }.to_string_lossy().to_string();
            return Err(format!("SMAppService uninstall failed: {}", err_msg));
        }

        Ok(())
    }
}

/// Check helper status via SMAppService Objective-C API
pub fn get_status() -> Result<HelperStatus, String> {
    #[cfg(not(target_os = "macos"))]
    { return Ok(HelperStatus { installed: false, running: false }); }

    #[cfg(target_os = "macos")]
    {
        let cls = objc2::class!(SMAppService);
        let service: *mut AnyObject = unsafe { objc2::msg_send![cls, mainAppService] };

        let status: isize = unsafe { objc2::msg_send![service, status] };

        // SMAppServiceStatus: 0=notRegistered, 1=registered, 2=enabled
        Ok(HelperStatus {
            installed: status >= 1,
            running: status == 2,
        })
    }
}
