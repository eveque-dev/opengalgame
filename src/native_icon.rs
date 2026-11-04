use anyhow::{Result, ensure};
use std::path::Path;
pub fn icon_png(path: &Path) -> Result<Vec<u8>> {
    let img = image::open(path)?.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let mut buffer = std::io::Cursor::new(vec![]);
    img.write_to(&mut buffer, image::ImageFormat::Png)?;
    Ok(buffer.into_inner())
}
#[cfg(windows)]
// Win32 MAKEINTRESOURCE uses the integer value as an opaque resource identifier.
#[allow(clippy::manual_dangling_ptr)]
pub fn set_exe_icon(exe: &Path, png: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
    };
    let data = icon_png(png)?;
    let path: Vec<_> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut group = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
    group.extend_from_slice(&(data.len() as u32).to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    // Only the newly exported executable is modified; preserve all unrelated resources.
    unsafe {
        let handle = BeginUpdateResourceW(path.as_ptr(), 0);
        ensure!(
            !handle.is_null(),
            "无法设置导出游戏图标：{}",
            std::io::Error::last_os_error()
        );
        let image_ok = UpdateResourceW(
            handle,
            3usize as *const u16,
            1usize as *const u16,
            0,
            data.as_ptr().cast(),
            data.len() as u32,
        ) != 0;
        let group_ok = UpdateResourceW(
            handle,
            14usize as *const u16,
            1usize as *const u16,
            0,
            group.as_ptr().cast(),
            group.len() as u32,
        ) != 0;
        let ok = EndUpdateResourceW(handle, if image_ok && group_ok { 0 } else { 1 }) != 0;
        ensure!(image_ok && group_ok && ok, "设置游戏图标失败");
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn set_exe_icon(_exe: &Path, _png: &Path) -> Result<()> {
    Ok(())
}
