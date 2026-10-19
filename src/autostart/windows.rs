use windows::core::w;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: windows::core::PCWSTR = w!("Lswi");

pub fn exe_path() -> Result<String, String> {
    unsafe {
        let mut buf = [0u16; 1024];
        let n = GetModuleFileNameW(None, &mut buf);
        if n == 0 {
            return Err("не удалось получить путь к программе".into());
        }
        Ok(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn apply(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_SET_VALUE, &mut key)
            .ok()
            .map_err(|e| format!("реестр: {e}"))?;
        let result = if enabled {
            let path = exe_path()?;
            let quoted = format!("\"{path}\"");
            let bytes: Vec<u8> = quoted
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|c| c.to_le_bytes())
                .collect();
            RegSetValueExW(key, VALUE_NAME, 0, REG_SZ, Some(&bytes))
                .ok()
                .map_err(|e| format!("запись автозапуска: {e}"))
        } else {
            match RegDeleteValueW(key, VALUE_NAME).ok() {
                Ok(()) => Ok(()),
                Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => Ok(()),
                Err(e) => Err(format!("удаление автозапуска: {e}")),
            }
        };
        let _ = RegCloseKey(key);
        result
    }
}
