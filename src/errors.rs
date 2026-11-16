// Keep system errors readable in English even on a localized Windows install.
pub fn io(error: std::io::Error) -> String {
    match error.raw_os_error() {
        Some(code) => format!("I/O error: {:?} (Windows error {code})", error.kind()),
        None => format!("I/O error: {:?}", error.kind()),
    }
}

pub fn windows(error: windows::core::Error) -> String {
    format!(
        "Windows API error (HRESULT 0x{:08X})",
        error.code().0 as u32
    )
}
