//! 控制台输出编码。
//!
//! 中文 Windows 的控制台默认按 936（GBK）解释字节，我们输出的是 UTF-8，不切一下就是一屏乱码。
//! 只在真要往控制台写字时临时切到 UTF-8，程序退出时还原，不改用户终端的长期设置，
//! 也不在转发 node/python 时切——那种场合要显示的是别人的输出，动代码页反而会把人家弄乱。

/// 拿着它期间控制台按 UTF-8 解释输出，丢弃时还原原来的代码页。
pub struct Guard {
    original: u32,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleOutputCP(self.original);
        }
    }
}

/// 输出真接到控制台、并且它现在不按 UTF-8 解释时切过去，返回还原句柄。
///
/// 返回 None 表示没动过：输出重定向到了文件、进程没连控制台，或者本来就是 UTF-8。
pub fn use_utf8() -> Option<Guard> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetConsoleOutputCP, GetStdHandle, SetConsoleOutputCP,
            STD_OUTPUT_HANDLE,
        };
        let original = unsafe { GetConsoleOutputCP() };
        let mut mode = 0;
        // 句柄无效或输出重定向到文件时 GetConsoleMode 直接返回 0，不用自己判空。
        let on_console =
            unsafe { GetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), &mut mode) } != 0;
        if !on_console || original == UTF8_CP || unsafe { SetConsoleOutputCP(UTF8_CP) } == 0 {
            return None;
        }
        Some(Guard { original })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
const UTF8_CP: u32 = 65001;
