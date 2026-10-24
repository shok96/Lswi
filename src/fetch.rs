use crate::util::wide;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Networking::WinInet::{
    HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile,
    InternetSetOptionW, HTTP_QUERY_CONTENT_LENGTH, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
    INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_RELOAD, INTERNET_FLAG_SECURE,
    INTERNET_OPEN_TYPE_DIRECT, INTERNET_OPEN_TYPE_PRECONFIG_WITH_NO_AUTOPROXY,
    INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_RECEIVE_TIMEOUT,
};

pub fn download_first(
    urls: &[&str],
    dest: &Path,
    label: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let mut last_err = "Нет адресов для скачивания".to_string();
    for (i, url) in urls.iter().enumerate() {
        if i == 0 {
            progress(&format!("Скачивание {label}…"));
        } else {
            progress(&format!("Другой сервер, скачивание {label}…"));
        }
        match download(url, dest, progress) {
            Ok(()) => return Ok(()),
            Err(err) => last_err = err,
        }
    }
    Err(last_err)
}

pub fn download(url: &str, dest: &Path, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    let tmp = dest.with_extension("part");
    let _ = fs::remove_file(&tmp);
    unsafe { wininet_download(url, &tmp, progress) }?;
    let size = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if size < 1024 {
        let _ = fs::remove_file(&tmp);
        return Err("Сервер вернул пустой файл".into());
    }
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

unsafe fn wininet_download(
    url: &str,
    dest: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let url_w = wide(url);
    let flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_SECURE;
    let access_types = [
        INTERNET_OPEN_TYPE_PRECONFIG_WITH_NO_AUTOPROXY,
        INTERNET_OPEN_TYPE_DIRECT.0,
    ];

    let mut last = "Не удалось открыть соединение".to_string();
    for access in access_types {
        let session = InternetOpenW(
            w!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) LSWI"),
            access,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            last = format!("Нет интернет-клиента Windows: {}", last_error());
            continue;
        }
        let _session = InetHandle(session);
        set_timeout(session, INTERNET_OPTION_CONNECT_TIMEOUT, 30_000);
        set_timeout(session, INTERNET_OPTION_RECEIVE_TIMEOUT, 300_000);

        let request = InternetOpenUrlW(session, PCWSTR(url_w.as_ptr()), None, flags, 0);
        if request.is_null() {
            last = format!("Нет сети или сервер недоступен: {}", last_error());
            continue;
        }
        let _request = InetHandle(request);
        set_timeout(request, INTERNET_OPTION_RECEIVE_TIMEOUT, 300_000);

        let status = query_u32(request, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER).unwrap_or(0);
        if status != 0 && !(200..300).contains(&status) {
            last = format!("Сервер вернул ошибку HTTP {status}");
            continue;
        }

        let total = query_u32(request, HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER)
            .map(u64::from)
            .unwrap_or(0);
        progress(&progress_text(0, total));

        let mut file = File::create(dest).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 64 * 1024];
        let mut done = 0u64;
        let mut last_pct = 0u64;
        loop {
            let mut read = 0u32;
            InternetReadFile(request, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read)
                .map_err(|e| format!("Обрыв загрузки: {e}"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read as usize]).map_err(|e| e.to_string())?;
            done += u64::from(read);
            let pct = if total > 0 {
                done.saturating_mul(100) / total
            } else {
                done / (256 * 1024)
            };
            if pct != last_pct {
                last_pct = pct;
                progress(&progress_text(done, total));
            }
        }
        file.flush().map_err(|e| e.to_string())?;
        if done < 1024 {
            last = "Сервер вернул пустой файл".into();
            continue;
        }
        return Ok(());
    }
    Err(last)
}

fn set_timeout(handle: *mut core::ffi::c_void, option: u32, ms: u32) {
    unsafe {
        let mut value = ms;
        let _ = InternetSetOptionW(
            Some(handle),
            option,
            Some((&mut value as *mut u32).cast()),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

unsafe fn query_u32(request: *mut core::ffi::c_void, query: u32) -> Option<u32> {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    HttpQueryInfoW(
        request,
        query,
        Some((&mut value as *mut u32).cast()),
        &mut size,
        None,
    )
    .ok()?;
    Some(value)
}

fn progress_text(done: u64, total: u64) -> String {
    if total > 0 {
        let pct = (done.saturating_mul(100) / total).min(99);
        format!(
            "Скачивание… {pct}% ({:.1} / {:.1} МБ)",
            done as f64 / 1_000_000.0,
            total as f64 / 1_000_000.0
        )
    } else {
        format!("Скачивание… {:.1} МБ", done as f64 / 1_000_000.0)
    }
}

pub fn ensure_cached(
    dir: &Path,
    file_name: &str,
    version: &str,
    urls: &[&str],
    label: &str,
    min_bytes: u64,
    progress: &mut dyn FnMut(&str),
) -> Result<std::path::PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("Не удалось создать папку: {e}"))?;
    let dest = dir.join(file_name);
    let marker = dir.join("VERSION");
    let ready = dest.exists()
        && fs::metadata(&dest).map(|m| m.len() >= min_bytes).unwrap_or(false)
        && fs::read_to_string(&marker)
            .map(|s| s.trim() == version)
            .unwrap_or(false);
    if ready {
        return Ok(dest);
    }
    let _ = fs::remove_file(&dest);
    download_first(urls, &dest, label, progress)?;
    if fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) < min_bytes {
        let _ = fs::remove_file(&dest);
        return Err(format!("{label}: файл повреждён"));
    }
    fs::write(&marker, version).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn last_error() -> String {
    unsafe { format!("{}", GetLastError().0) }
}

struct InetHandle(*mut core::ffi::c_void);

impl Drop for InetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = InternetCloseHandle(self.0);
            }
        }
    }
}
