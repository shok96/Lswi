use windows::Win32::Foundation::HWND;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn hwnd_to_isize(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

pub fn hwnd_from_isize(value: isize) -> HWND {
    HWND(value as *mut core::ffi::c_void)
}

pub fn copy_wide(dest: &mut [u16], src: &str) {
    dest.fill(0);
    for (slot, ch) in dest.iter_mut().zip(src.encode_utf16()) {
        *slot = ch;
    }
    if !dest.is_empty() {
        dest[dest.len() - 1] = 0;
    }
}
