#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::apply;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::apply;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::apply;
