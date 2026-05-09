//! Windows 系统托盘模块 — 程序化绘制手柄图标

use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

const WM_TRAYICON: u32 = WM_USER + 1;
const ID_TRAY: u32 = 1;
const IDM_EXIT: usize = 1001;
const IDM_RELOAD: usize = 1002;
const IDM_TOGGLE_PET: usize = 1003;
const ICON_SIZE: i32 = 32;

pub enum TrayCommand {
    Exit,
    Reload,
    TogglePet,
}

static mut TX: usize = 0;

/// RGB 颜色宏
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

pub fn run(tx: std::sync::mpsc::Sender<TrayCommand>) -> Result<(), String> {
    unsafe {
        // DPI 感知：让菜单和图标在高分屏上清晰渲染
        let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
        let class_name = w!("ai-pad-tray");
        let module = GetModuleHandleW(null());

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: module,
            lpszClassName: class_name,
            ..Default::default()
        };

        if RegisterClassW(&wnd_class) == 0 {
            return Err("RegisterClassW failed".into());
        }

        let hwnd = CreateWindowExW(
            0, class_name, w!("ai-pad"),
            0, 0, 0, 0, 0,
            HWND_MESSAGE,
            null_mut(), module, null_mut(),
        );

        if hwnd.is_null() {
            return Err("CreateWindowExW failed".into());
        }

        TX = Box::into_raw(Box::new(tx)) as usize;

        let icon = create_gamepad_icon();
        let tip = w!("ai-pad: 手柄控制器");
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ID_TRAY,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            ..Default::default()
        };
        copy_wide_str(tip, &mut nid.szTip);

        if Shell_NotifyIconW(NIM_ADD, &mut nid) == 0 {
            return Err("Shell_NotifyIconW failed".into());
        }

        let mut msg = std::mem::zeroed();
        loop {
            let ret = GetMessageW(&mut msg, null_mut(), 0, 0);
            if ret == 0 || ret == -1 { break; }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Shell_NotifyIconW(NIM_DELETE, &mut nid);
        DestroyIcon(icon);

        if TX != 0 {
            drop(Box::from_raw(TX as *mut std::sync::mpsc::Sender<TrayCommand>));
            TX = 0;
        }

        Ok(())
    }
}

/// 用 GDI 绘制一个 32x32 的手柄图标
unsafe fn create_gamepad_icon() -> HICON {
    unsafe {
        let screen_dc = GetDC(null_mut());
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, ICON_SIZE, ICON_SIZE);
        let old_bmp = SelectObject(mem_dc, bmp);

        // 背景：白色
        let bg_brush = CreateSolidBrush(rgb(255, 255, 255));
        Rectangle(mem_dc, 0, 0, ICON_SIZE, ICON_SIZE);
        DeleteObject(bg_brush);

        // 手柄主体：圆角矩形（蓝色）
        let body_brush = CreateSolidBrush(rgb(59, 130, 246));
        let pen = CreatePen(PS_SOLID, 1, rgb(37, 99, 235));
        SelectObject(mem_dc, pen);
        SelectObject(mem_dc, body_brush);
        RoundRect(mem_dc, 2, 7, 30, 27, 10, 10);
        DeleteObject(pen);
        DeleteObject(body_brush);

        // 左摇杆
        let stick_brush = CreateSolidBrush(rgb(30, 58, 138));
        SelectObject(mem_dc, stick_brush);
        Ellipse(mem_dc, 5, 12, 13, 20);
        // 右摇杆
        Ellipse(mem_dc, 19, 14, 27, 22);
        DeleteObject(stick_brush);

        // 十字方向键
        let dpad_pen = CreatePen(PS_SOLID, 1, rgb(255, 255, 255));
        SelectObject(mem_dc, dpad_pen);
        MoveToEx(mem_dc, 9, 16, null_mut()); LineTo(mem_dc, 15, 16);
        MoveToEx(mem_dc, 12, 13, null_mut()); LineTo(mem_dc, 12, 19);
        DeleteObject(dpad_pen);

        // ABXY 四色按钮
        let btn_colors = [
            rgb(239, 68, 68),
            rgb(34, 197, 94),
            rgb(250, 204, 21),
            rgb(96, 165, 250),
        ];
        let btn_pos = [(24, 11), (28, 15), (20, 15), (24, 19)];
        for (i, &(x, y)) in btn_pos.iter().enumerate() {
            let b = CreateSolidBrush(btn_colors[i]);
            SelectObject(mem_dc, b);
            Ellipse(mem_dc, x, y, x + 4, y + 4);
            DeleteObject(b);
        }

        SelectObject(mem_dc, old_bmp);
        ReleaseDC(null_mut(), screen_dc);

        // mask 位图
        let mask_dc = CreateCompatibleDC(null_mut());
        let mask_bmp = CreateBitmap(ICON_SIZE, ICON_SIZE, 1, 1, null_mut());
        let old_mask = SelectObject(mask_dc, mask_bmp);
        let mask_brush = CreateSolidBrush(rgb(0, 0, 0));
        SelectObject(mask_dc, mask_brush);
        Ellipse(mask_dc, -1, -1, ICON_SIZE + 1, ICON_SIZE + 1);
        DeleteObject(mask_brush);
        SelectObject(mask_dc, old_mask);
        DeleteDC(mask_dc);

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 16,
            yHotspot: 16,
            hbmColor: bmp,
            hbmMask: mask_bmp,
        };
        let icon = CreateIconIndirect(&info);

        DeleteDC(mem_dc);
        DeleteObject(bmp);
        DeleteObject(mask_bmp);

        icon
    }
}

unsafe fn send_cmd(cmd: TrayCommand) {
    unsafe {
        if TX != 0 {
            let tx = &*(TX as *const std::sync::mpsc::Sender<TrayCommand>);
            let _ = tx.send(cmd);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            match lparam as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => unsafe { show_menu(hwnd) },
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xFFFF;
            match id as usize {
                IDM_EXIT => unsafe {
                    send_cmd(TrayCommand::Exit);
                    PostQuitMessage(0);
                },
                IDM_RELOAD => unsafe { send_cmd(TrayCommand::Reload) },
                IDM_TOGGLE_PET => unsafe { send_cmd(TrayCommand::TogglePet) },
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, IDM_RELOAD, w!("重载配置"));
        AppendMenuW(menu, MF_STRING, IDM_TOGGLE_PET, w!("显示/隐藏宠物"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, IDM_EXIT, w!("退出"));

        let mut pt = std::mem::zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(menu);
    }
}

unsafe fn copy_wide_str(src: *const u16, dst: &mut [u16; 128]) {
    unsafe {
        for i in 0..128 {
            let c = *src.add(i);
            dst[i] = c;
            if c == 0 { break; }
        }
    }
}

/// 显示系统消息框（用于无控制台时的错误提示）
pub fn show_error(title: &str, msg: &str) {
    unsafe {
        let t = to_wide(title);
        let m = to_wide(msg);
        MessageBoxW(null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
