#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod error;
mod i18n;
mod settings;
mod system;
mod theme;

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT,
    WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, HGDIOBJ,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, SetWindowRgn,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClientRect, GetWindowRect, IsZoomed, NCCALCSIZE_PARAMS, SPI_GETWORKAREA,
    SW_RESTORE, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow,
    SetWindowPos, ShowWindow, SystemParametersInfoW, WM_NCCALCSIZE, WM_SIZE,
};
use windows::core::{HSTRING, PCWSTR};

use crate::error::CommandError;
use crate::i18n::I18n;
use crate::settings::Settings;

slint::include_modules!();

const APP_VERSION: &str = "0.1.0";
const REFRESH_INTERVALS: [u64; 4] = [2, 3, 5, 10];
const ACCENTS: [&str; 4] = ["blue", "teal", "purple", "amber"];
const THEMES: [&str; 3] = ["system", "light", "dark"];

thread_local! {
    static TRAY: RefCell<Option<AppTray>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorKind {
    Devices,
    Connect,
}

struct UiState {
    devices: Vec<audio::DeviceDto>,
    selected: Option<String>,
    active: Option<String>,
    connecting: bool,
    closing: bool,
    loading: bool,
    polling: bool,
    error: Option<CommandError>,
    error_kind: Option<ErrorKind>,
    settings: Settings,
    i18n: I18n,
    system_dark: bool,
    dark: bool,
    next_refresh: Instant,
    next_poll: Instant,
    next_theme_check: Instant,
    next_region_check: Instant,
    snackbar_until: Option<Instant>,
    center_at: Instant,
    window_centered: bool,
    last_device_signature: RefCell<String>,
}

impl UiState {
    fn new(settings: Settings) -> Self {
        let system_dark = theme::system_dark();
        let dark = resolve_dark(&settings.theme, system_dark);
        let i18n = I18n::load(&settings.language);
        Self {
            devices: Vec::new(),
            selected: None,
            active: None,
            connecting: false,
            closing: false,
            loading: false,
            polling: false,
            error: None,
            error_kind: None,
            settings,
            i18n,
            system_dark,
            dark,
            next_refresh: Instant::now(),
            next_poll: Instant::now(),
            next_theme_check: Instant::now(),
            next_region_check: Instant::now(),
            snackbar_until: None,
            center_at: Instant::now() + Duration::from_millis(400),
            window_centered: false,
            last_device_signature: RefCell::new(String::new()),
        }
    }
}

struct SingleInstanceGuard(HANDLE);

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    let instance_name = std::env::var("AUXLANE_INSTANCE")
        .unwrap_or_else(|_| "Local\\Auxlane.SingleInstance".to_string());
    let instance_name = HSTRING::from(instance_name);
    unsafe {
        match CreateMutexW(None, false, PCWSTR(instance_name.as_ptr())) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    if let Ok(window) = FindWindowW(None, windows::core::w!("Auxlane")) {
                        let _ = ShowWindow(window, SW_RESTORE);
                        let _ = SetForegroundWindow(window);
                    }
                    let _ = CloseHandle(handle);
                    None
                } else {
                    Some(SingleInstanceGuard(handle))
                }
            }
            Err(error) => {
                log::warn!("Could not create single-instance mutex: {error}");
                Some(SingleInstanceGuard(HANDLE::default()))
            }
        }
    }
}

fn init_logging() {
    if let Some(directory) = dirs::data_local_dir().map(|path| path.join("Auxlane").join("logs")) {
        let _ = std::fs::create_dir_all(&directory);
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("auxlane.log"))
        {
            let _ = simplelog::WriteLogger::init(
                log::LevelFilter::Info,
                simplelog::Config::default(),
                file,
            );
        }
    }
    log::info!("Auxlane {APP_VERSION} starting");
}

fn resolve_dark(preference: &str, system_dark: bool) -> bool {
    match preference {
        "light" => false,
        "dark" => true,
        _ => system_dark,
    }
}

fn theme_index(preference: &str) -> i32 {
    THEMES
        .iter()
        .position(|theme| *theme == preference)
        .unwrap_or(0) as i32
}

fn refresh_index(seconds: u64) -> i32 {
    REFRESH_INTERVALS
        .iter()
        .position(|value| *value == seconds)
        .unwrap_or(1) as i32
}

fn accent_index(accent: &str) -> i32 {
    ACCENTS
        .iter()
        .position(|value| *value == accent)
        .unwrap_or(0) as i32
}

fn main() -> Result<(), slint::PlatformError> {
    let _single_instance = match acquire_single_instance() {
        Some(guard) => guard,
        None => return Ok(()),
    };

    init_logging();

    let state = Arc::new(Mutex::new(UiState::new(settings::load())));
    let backend = Arc::new(audio::AppState::default());

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        // Create the window borderless from the first frame. Without this,
        // winit can briefly create a decorated window and Windows (especially
        // without DWM composition) paints a classic Win7-style title bar.
        .with_winit_window_attributes_hook(|attributes| attributes.with_decorations(false))
        .select()?;

    let ui = AppWindow::new()?;
    ui.set_app_version(APP_VERSION.into());
    ui.window().set_size(slint::LogicalSize::new(1000.0, 640.0));

    // Closing the window hides it to the tray; connections stay alive. The
    // tray icon keeps Slint's event loop running. If the tray could not be
    // created, hiding the last window makes run_event_loop() return and the
    // app exits after close_all() below.
    ui.window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    match AppTray::new() {
        Ok(tray) => {
            let ui_weak = ui.as_weak();
            tray.on_show_window(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    show_window(&ui);
                }
            });
            {
                let ui_weak = ui.as_weak();
                let state = state.clone();
                let backend = backend.clone();
                tray.on_disconnect(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        disconnect(&ui, &state, &backend);
                    }
                });
            }
            {
                let backend = backend.clone();
                tray.on_quit(move || {
                    backend.close_all();
                    let _ = slint::quit_event_loop();
                });
            }
            if let Err(error) = tray.show() {
                log::warn!("Could not show system tray: {error}");
            }
            TRAY.with(|slot| *slot.borrow_mut() = Some(tray));
            log::info!("System tray ready");
        }
        Err(error) => {
            log::warn!("System tray unavailable: {error}");
        }
    }

    apply_all(&ui, &state);
    wire(&ui, &state, &backend);

    let tick = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let backend = backend.clone();
        tick.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            if let Some(ui) = ui_weak.upgrade() {
                tick_app(&ui, &state, &backend);
            }
        });
    }

    ui.show()?;
    // The winit window is created lazily inside the event loop; the timer
    // installs the borderless window procedure and DWM corner preference on
    // its first tick (see `ensure_window_chrome`).
    let result = slint::run_event_loop();
    backend.close_all();
    result
}

fn with_window_handle<R>(ui: &AppWindow, f: impl FnOnce(HWND) -> R) -> Option<R> {
    let mut result = None;
    ui.window()
        .with_winit_window(|window: &slint::winit_030::winit::window::Window| {
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(raw) = handle.as_raw()
            {
                result = Some(f(HWND(raw.hwnd.get() as *mut core::ffi::c_void)));
            }
        });
    result
}

fn show_window(ui: &AppWindow) {
    let _ = ui.show();
    ui.window().set_minimized(false);
    with_window_handle(ui, |window| unsafe {
        let _ = ShowWindow(window, SW_RESTORE);
        let _ = SetForegroundWindow(window);
    });
}

const BORDERLESS_SUBCLASS_ID: usize = 0x4155_584C; // "AUXL"
static WINDOW_CHROME_READY: AtomicBool = AtomicBool::new(false);
static DWM_CORNERS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Windows paints a native title bar over a custom-chrome window when the
/// non-client area changes on a live window, and a maximized window whose top
/// frame is only partially covered gets a "phantom" caption from DWM. This
/// used to show up as the classic "Windows 7 title bar" across the custom
/// header: winit keeps `WS_CAPTION` on top-level windows and hides the
/// non-client area in `WM_NCCALCSIZE`, while the app cleared `WS_CAPTION`
/// every 100 ms. That style churn is what made DWM paint the caption.
///
/// Keep the native frame styles (DWM needs them for rounded corners, shadows
/// and animations) and own `WM_NCCALCSIZE` instead: the client area always
/// covers the whole window. When maximized, the window rectangle overhangs the
/// monitor by the resize frame; keep the overhang at the top so DWM has no
/// caption area to paint, align the sides with the work area, and stop at the
/// taskbar at the bottom.
unsafe extern "system" fn borderless_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    match message {
        WM_NCCALCSIZE if wparam.0 != 0 => {
            let params = unsafe { &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS) };
            if unsafe { IsZoomed(hwnd) }.as_bool()
                && let Some(work_area) = monitor_work_area(hwnd)
            {
                // `rgrc[0].top` is the top of the maximized window rectangle,
                // which overhangs the monitor. Leaving it there keeps the
                // caption fully covered; the sides and bottom follow the work
                // area so the client does not cover the taskbar.
                params.rgrc[0].left = work_area.left;
                params.rgrc[0].right = work_area.right;
                params.rgrc[0].bottom = work_area.bottom;
            }
            LRESULT(0)
        }
        WM_SIZE => {
            // Drop or restore the custom rounded region as soon as the
            // maximize state changes instead of waiting for the next tick,
            // which would briefly clip the maximized window to the old
            // rounded region.
            let maximized = unsafe { IsZoomed(hwnd) }.as_bool();
            let changed = LAST_REGION
                .lock()
                .is_ok_and(|last| last.is_some_and(|key: RegionKey| key.maximized != maximized));
            if changed {
                apply_rounded_region_hwnd(hwnd, true);
            }
            unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
        }
        _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}

fn monitor_work_area(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            Some(info.rcWork)
        } else {
            None
        }
    }
}

/// Slint creates the winit window lazily inside the event loop, so this has
/// to be (re)tried from a timer callback rather than right after `show()`.
fn ensure_window_chrome(ui: &AppWindow) {
    if WINDOW_CHROME_READY.load(Ordering::Relaxed) {
        return;
    }

    let setup = with_window_handle(ui, |window| unsafe {
        let subclassed = SetWindowSubclass(
            window,
            Some(borderless_window_proc),
            BORDERLESS_SUBCLASS_ID,
            0,
        )
        .as_bool();

        let preference = DWMWCP_ROUND;
        let dwm_corners = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&preference) as u32,
        )
        .is_ok();

        (subclassed, dwm_corners)
    });
    let Some((subclassed, dwm_corners)) = setup else {
        // No HWND yet; try again on the next tick.
        return;
    };

    WINDOW_CHROME_READY.store(true, Ordering::Relaxed);
    DWM_CORNERS_ACTIVE.store(dwm_corners, Ordering::Relaxed);
    if !subclassed {
        log::warn!("Could not install the borderless window procedure");
    }
    if dwm_corners {
        log::info!("Rounded window corners enabled");
    } else {
        log::info!("Using a rounded window region (DWM corner rounding unavailable)");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RegionKey {
    width: i32,
    height: i32,
    dpi: u32,
    offset_x: i32,
    offset_y: i32,
    maximized: bool,
}

static LAST_REGION: Mutex<Option<RegionKey>> = Mutex::new(None);

/// DWM rounded corners are not applied to every frameless window style, so
/// also clip the client area to a rounded region. The region is refreshed
/// when the client size changes.
fn apply_rounded_region(ui: &AppWindow, force: bool) {
    with_window_handle(ui, |window| apply_rounded_region_hwnd(window, force));
}

fn apply_rounded_region_hwnd(window: HWND, force: bool) {
    // Windows 11 rounds top-level windows natively. The custom region is a
    // Windows 10 fallback; on Win11 it would only fight DWM's own clipping and
    // briefly clip the window while it is being maximized.
    if DWM_CORNERS_ACTIVE.load(Ordering::Relaxed) {
        return;
    }

    let mut client = RECT::default();
    if unsafe { GetClientRect(window, &mut client) }.is_err() {
        return;
    }
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width <= 0 || height <= 0 {
        return;
    }

    let dpi = unsafe { GetDpiForWindow(window) };
    let maximized = unsafe { IsZoomed(window) }.as_bool();
    let mut origin = POINT { x: 0, y: 0 };
    let _ = unsafe { ClientToScreen(window, &mut origin) };
    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(window, &mut window_rect) }.is_err() {
        return;
    }
    let offset_x = origin.x - window_rect.left;
    let offset_y = origin.y - window_rect.top;

    // Include the client offset: a frameless window can briefly have a
    // non-client area, and a stale offset would leave a strip at the top.
    let key = RegionKey {
        width,
        height,
        dpi,
        offset_x,
        offset_y,
        maximized,
    };
    {
        let Ok(mut last) = LAST_REGION.lock() else {
            return;
        };
        if !force && *last == Some(key) {
            return;
        }
        *last = Some(key);
    }

    if maximized {
        // Windows does not round maximized windows, and the maximized client
        // area overhangs the screen edges; a stale region would clip the
        // window to its previous, smaller shape.
        unsafe {
            SetWindowRgn(window, None, true);
        }
        return;
    }

    let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
    let ellipse = (32.0 * scale).round().max(8.0) as i32;
    let region = unsafe {
        CreateRoundRectRgn(
            offset_x,
            offset_y,
            offset_x + width + 1,
            offset_y + height + 1,
            ellipse,
            ellipse,
        )
    };
    if region.is_invalid() {
        return;
    }
    if unsafe { SetWindowRgn(window, Some(region), true) } == 0 {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(region.0));
        }
    }
}

fn center_window(ui: &AppWindow) {
    with_window_handle(ui, |window| {
        let mut window_rect = RECT::default();
        if unsafe { GetWindowRect(window, &mut window_rect) }.is_err() {
            return;
        }
        let mut work_area = RECT::default();
        if unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut _ as *mut core::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .is_err()
        {
            return;
        }

        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        let x = work_area.left + ((work_area.right - work_area.left - width) / 2);
        let y = work_area.top + ((work_area.bottom - work_area.top - height) / 2);
        let _ = unsafe { SetWindowPos(window, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER) };
    });
}

fn apply_all(ui: &AppWindow, state: &Arc<Mutex<UiState>>) {
    let (dark, accent, texts) = {
        let locked = state.lock().unwrap();
        (
            locked.dark,
            locked.settings.accent.clone(),
            build_texts(&locked.i18n),
        )
    };
    theme::apply(ui, dark, &accent);
    ui.set_texts(texts);
    sync_settings_controls(ui, state);
    let locked = state.lock().unwrap();
    render(ui, &locked);
}

fn sync_settings_controls(ui: &AppWindow, state: &Arc<Mutex<UiState>>) {
    let locked = state.lock().unwrap();
    ui.set_theme_preference(theme_index(&locked.settings.theme));
    ui.set_language_preference(i18n::language_index(&locked.settings.language));
    ui.set_refresh_index(refresh_index(locked.settings.refresh_seconds()));
    ui.set_accent_index(accent_index(&locked.settings.accent));
}

fn build_texts(i18n: &I18n) -> Texts {
    Texts {
        subtitle: i18n.t("app.subtitle").into(),
        devices: i18n.t("devices.label").into(),
        add: i18n.t("devices.add").into(),
        scanning: i18n.t("device.scanningDesc").into(),
        empty: i18n.t("devices.empty").into(),
        empty_hint: i18n.t("devices.emptyHint").into(),
        device_selected: i18n.t("device.selected").into(),
        device_ready: i18n.t("device.readyReceive").into(),
        device_unavailable: i18n.t("device.unavailable").into(),
        device_streaming: i18n.t("device.streaming").into(),
        settings_title: i18n.t("settings.title").into(),
        settings_general: i18n.t("settings.general").into(),
        settings_appearance: i18n.t("settings.appearance").into(),
        settings_about: i18n.t("settings.about").into(),
        settings_theme: i18n.t("settings.theme").into(),
        settings_theme_desc: i18n.t("settings.themeDesc").into(),
        theme_system: i18n.t("settings.system").into(),
        theme_light: i18n.t("settings.light").into(),
        theme_dark: i18n.t("settings.dark").into(),
        settings_refresh: i18n.t("settings.refresh").into(),
        settings_refresh_desc: i18n.t("settings.refreshDesc").into(),
        seconds_2: i18n.t("settings.seconds2").into(),
        seconds_3: i18n.t("settings.seconds3").into(),
        seconds_5: i18n.t("settings.seconds5").into(),
        seconds_10: i18n.t("settings.seconds10").into(),
        settings_language: i18n.t("settings.language").into(),
        settings_language_desc: i18n.t("settings.languageDesc").into(),
        language_system: i18n.t("language.system").into(),
        settings_accent: i18n.t("settings.accent").into(),
        settings_accent_desc: i18n.t("settings.accentDesc").into(),
        accent_blue: i18n.t("accent.blue").into(),
        accent_teal: i18n.t("accent.teal").into(),
        accent_purple: i18n.t("accent.purple").into(),
        accent_amber: i18n.t("accent.amber").into(),
        version_label: i18n.t("settings.versionLabel").into(),
        design: i18n.t("settings.design").into(),
        design_desc: i18n.t("settings.designDesc").into(),
        built_with: i18n.t("settings.builtWith").into(),
        built_with_desc: i18n.t("settings.builtWithDesc").into(),
        aria_minimize: i18n.t("aria.minimize").into(),
        aria_close: i18n.t("aria.close").into(),
        aria_theme_dark: i18n.t("aria.themeDark").into(),
        aria_theme_light: i18n.t("aria.themeLight").into(),
        aria_refresh: i18n.t("aria.refreshDevices").into(),
        aria_settings: i18n.t("aria.settings").into(),
        aria_close_settings: i18n.t("aria.closeSettings").into(),
        aria_add_device: i18n.t("aria.addDevice").into(),
        aria_paired_devices: i18n.t("aria.pairedDevices").into(),
        snackbar_dismiss: i18n.t("snackbar.dismiss").into(),
        stage_idle: i18n.t("stage.idleHint").into(),
        stage_connected: i18n.t("stage.connectedHint").into(),
        more_title: i18n.t("more.title").into(),
        aria_more: i18n.t("aria.more").into(),
        aria_close_more: i18n.t("aria.close-more").into(),
    }
}

fn wire(ui: &AppWindow, state: &Arc<Mutex<UiState>>, backend: &Arc<audio::AppState>) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_refresh(move || {
            if let Some(ui) = ui_weak.upgrade() {
                refresh(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_add_device(move || {
            if let Some(ui) = ui_weak.upgrade() {
                add_device(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_select(move |device_id| {
            if let Some(ui) = ui_weak.upgrade() {
                select_device(&ui, &state, device_id.as_str());
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let backend = backend.clone();
        ui.on_connect(move |device_id| {
            if let Some(ui) = ui_weak.upgrade() {
                connect(&ui, &state, &backend, device_id.to_string());
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let backend = backend.clone();
        ui.on_disconnect(move || {
            if let Some(ui) = ui_weak.upgrade() {
                disconnect(&ui, &state, &backend);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_toggle_favorite(move |device_id| {
            if let Some(ui) = ui_weak.upgrade() {
                toggle_favorite(&ui, &state, device_id.as_str());
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let backend = backend.clone();
        ui.on_primary_action(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let (active, selected) = {
                    let locked = state.lock().unwrap();
                    (locked.active.clone(), locked.selected.clone())
                };
                if active.is_some() {
                    disconnect(&ui, &state, &backend);
                } else if let Some(device_id) = selected {
                    connect(&ui, &state, &backend, device_id);
                } else {
                    let message = {
                        let locked = state.lock().unwrap();
                        locked.i18n.t("snackbar.selectDeviceFirst")
                    };
                    show_snackbar(&ui, &state, message);
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_theme_toggle(move || {
            if let Some(ui) = ui_weak.upgrade() {
                toggle_theme(&ui, &state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_open_settings(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_more_open(false);
                ui.set_settings_section(0);
                ui.set_settings_open(true);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_close_settings(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_settings_open(false);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_set_settings_section(move |section| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_settings_section(section);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_set_theme(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                set_theme(&ui, &state, index);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_set_language(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                set_language(&ui, &state, index);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_set_refresh(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                set_refresh(&ui, &state, index);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_set_accent(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                set_accent(&ui, &state, index);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.on_dismiss_snackbar(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut locked = state.lock().unwrap();
                locked.snackbar_until = None;
                ui.set_snackbar_visible(false);
            }
        });
    }
}

fn tick_app(ui: &AppWindow, state: &Arc<Mutex<UiState>>, backend: &Arc<audio::AppState>) {
    ensure_window_chrome(ui);

    let now = Instant::now();
    let center_window_now = {
        let mut locked = state.lock().unwrap();
        if !locked.window_centered && now >= locked.center_at {
            locked.window_centered = true;
            true
        } else {
            false
        }
    };
    if center_window_now {
        center_window(ui);
        // Apply the region only after the window has its final size. Doing it
        // immediately after show() can use a tiny pre-layout rect.
        apply_rounded_region(ui, true);
    }

    let mut refresh_now = false;
    let mut poll_device: Option<String> = None;
    let mut hide_snackbar = false;
    let mut check_theme = false;
    let mut check_region = false;
    let mut ring_phase: Option<f32> = None;

    {
        let mut locked = state.lock().unwrap();

        if locked.active.is_some() {
            let phase = ui.get_ring_phase();
            ring_phase = Some((phase + 18.0) % 360.0);
        } else if ui.get_ring_phase() != 0.0 {
            ring_phase = Some(0.0);
        }

        if locked
            .snackbar_until
            .is_some_and(|deadline| now >= deadline)
        {
            locked.snackbar_until = None;
            hide_snackbar = true;
        }

        if now >= locked.next_refresh && !locked.loading {
            locked.next_refresh = now + Duration::from_secs(locked.settings.refresh_seconds());
            refresh_now = true;
        }

        if locked.active.is_some() && !locked.polling && now >= locked.next_poll {
            locked.polling = true;
            locked.next_poll = now + Duration::from_millis(800);
            poll_device = locked.active.clone();
        }

        if locked.settings.theme == "system" && now >= locked.next_theme_check {
            locked.next_theme_check = now + Duration::from_secs(2);
            check_theme = true;
        }

        if locked.window_centered && now >= locked.next_region_check {
            locked.next_region_check = now + Duration::from_millis(100);
            check_region = true;
        }
    }

    if check_region {
        // Keep the rounded region in sync with moves across monitors (DPI)
        // and with the maximize state.
        apply_rounded_region(ui, false);
    }

    if let Some(phase) = ring_phase {
        ui.set_ring_phase(phase);
    }
    if hide_snackbar {
        ui.set_snackbar_visible(false);
    }
    if check_theme {
        let system_dark = theme::system_dark();
        let changed = {
            let mut locked = state.lock().unwrap();
            if locked.system_dark != system_dark {
                locked.system_dark = system_dark;
                locked.dark = resolve_dark(&locked.settings.theme, system_dark);
                true
            } else {
                false
            }
        };
        if changed {
            let (dark, accent) = {
                let locked = state.lock().unwrap();
                (locked.dark, locked.settings.accent.clone())
            };
            theme::apply(ui, dark, &accent);
        }
    }
    if refresh_now {
        refresh(ui, state);
    }
    if let Some(device_id) = poll_device {
        poll_connection(ui, state, backend, device_id);
    }
}

fn refresh(ui: &AppWindow, state: &Arc<Mutex<UiState>>) {
    {
        let mut locked = state.lock().unwrap();
        if locked.loading {
            return;
        }
        locked.loading = true;
        render(ui, &locked);
    }

    let ui_weak = ui.as_weak();
    let state = state.clone();
    std::thread::spawn(move || {
        let result = audio::list_devices();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut locked = state.lock().unwrap();
                locked.loading = false;
                locked.next_refresh =
                    Instant::now() + Duration::from_secs(locked.settings.refresh_seconds());

                let mut snackbar_message = None;
                match result {
                    Ok(devices) => {
                        let selected_exists = locked
                            .selected
                            .as_ref()
                            .is_some_and(|id| devices.iter().any(|device| &device.id == id));
                        if !selected_exists {
                            locked.selected = devices
                                .iter()
                                .find(|device| device.is_enabled)
                                .map(|device| device.id.clone())
                                .or_else(|| devices.first().map(|device| device.id.clone()));
                        }
                        locked.devices = devices;
                        if locked.error_kind == Some(ErrorKind::Devices) {
                            locked.error = None;
                            locked.error_kind = None;
                        }
                    }
                    Err(error) => {
                        locked.error = Some(error);
                        locked.error_kind = Some(ErrorKind::Devices);
                        if let Some(error) = &locked.error {
                            snackbar_message = Some(localize_error(&locked.i18n, error));
                        }
                    }
                }
                render(&ui, &locked);
                drop(locked);
                if let Some(message) = snackbar_message {
                    show_snackbar(&ui, &state, message);
                }
            }
        });
    });
}

fn poll_connection(
    ui: &AppWindow,
    state: &Arc<Mutex<UiState>>,
    backend: &Arc<audio::AppState>,
    device_id: String,
) {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    let backend = backend.clone();
    std::thread::spawn(move || {
        let is_open = audio::connection_is_open(&backend, &device_id);
        let closed = matches!(is_open, Ok(false));
        if closed {
            let _ = audio::close_connection(&backend, &device_id);
        }
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut locked = state.lock().unwrap();
                locked.polling = false;
                if closed && locked.active.as_deref() == Some(device_id.as_str()) {
                    let name = device_name(&locked, &device_id);
                    locked.active = None;
                    locked.connecting = false;
                    locked.closing = false;
                    locked.error = None;
                    locked.error_kind = None;
                    render(&ui, &locked);
                    let message = locked
                        .i18n
                        .t_args("snackbar.deviceDisconnected", &[("name", &name)]);
                    show_snackbar_locked(&ui, &mut locked, message);
                }
            }
        });
    });
}

fn select_device(ui: &AppWindow, state: &Arc<Mutex<UiState>>, device_id: &str) {
    let mut locked = state.lock().unwrap();
    if locked.connecting || locked.closing {
        return;
    }
    if locked.active.is_some() {
        if locked.active.as_deref() != Some(device_id) {
            let message = locked.i18n.t("snackbar.closeFirst");
            show_snackbar_locked(ui, &mut locked, message);
        }
        return;
    }
    locked.selected = Some(device_id.to_string());
    locked.error = None;
    locked.error_kind = None;
    render(ui, &locked);
}

fn connect(
    ui: &AppWindow,
    state: &Arc<Mutex<UiState>>,
    backend: &Arc<audio::AppState>,
    device_id: String,
) {
    {
        let mut locked = state.lock().unwrap();
        if locked.connecting || locked.closing || locked.active.is_some() {
            return;
        }
        locked.selected = Some(device_id.clone());
        locked.connecting = true;
        locked.error = None;
        locked.error_kind = None;
        render(ui, &locked);
    }

    let ui_weak = ui.as_weak();
    let state = state.clone();
    let backend = backend.clone();
    std::thread::spawn(move || {
        let result = audio::open_connection(&backend, &device_id);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut locked = state.lock().unwrap();
                locked.connecting = false;
                match result {
                    Ok(()) => {
                        let name = device_name(&locked, &device_id);
                        locked.active = Some(device_id.clone());
                        locked.error = None;
                        locked.error_kind = None;
                        render(&ui, &locked);
                        ui.set_ring_phase(0.0);
                        let message = locked.i18n.t_args("snackbar.connected", &[("name", &name)]);
                        show_snackbar_locked(&ui, &mut locked, message);
                    }
                    Err(error) => {
                        let message = localize_error(&locked.i18n, &error);
                        locked.error = Some(error);
                        locked.error_kind = Some(ErrorKind::Connect);
                        render(&ui, &locked);
                        show_snackbar_locked(&ui, &mut locked, message);
                    }
                }
            }
        });
    });
}

fn disconnect(ui: &AppWindow, state: &Arc<Mutex<UiState>>, backend: &Arc<audio::AppState>) {
    let (device_id, name) = {
        let mut locked = state.lock().unwrap();
        if locked.connecting || locked.closing || locked.active.is_none() {
            return;
        }
        locked.closing = true;
        locked.error = None;
        locked.error_kind = None;
        render(ui, &locked);
        let device_id = locked.active.clone().unwrap();
        let name = device_name(&locked, &device_id);
        (device_id, name)
    };

    let ui_weak = ui.as_weak();
    let state = state.clone();
    let backend = backend.clone();
    std::thread::spawn(move || {
        let result = audio::close_connection(&backend, &device_id);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut locked = state.lock().unwrap();
                locked.closing = false;
                locked.active = None;
                match result {
                    Ok(()) => {
                        render(&ui, &locked);
                        let message = locked
                            .i18n
                            .t_args("snackbar.disconnected", &[("name", &name)]);
                        show_snackbar_locked(&ui, &mut locked, message);
                    }
                    Err(error) => {
                        let message = localize_error(&locked.i18n, &error);
                        locked.error = Some(error);
                        locked.error_kind = Some(ErrorKind::Connect);
                        render(&ui, &locked);
                        show_snackbar_locked(&ui, &mut locked, message);
                    }
                }
            }
        });
    });
}

fn add_device(ui: &AppWindow, state: &Arc<Mutex<UiState>>) {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    std::thread::spawn(move || {
        let result = system::open_bluetooth_settings();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let locked = state.lock().unwrap();
                let message = match &result {
                    Ok(()) => locked.i18n.t("snackbar.pairHint"),
                    Err(error) => localize_error(&locked.i18n, error),
                };
                drop(locked);
                show_snackbar(&ui, &state, message);
            }
        });
    });
}

fn toggle_theme(ui: &AppWindow, state: &Arc<Mutex<UiState>>) {
    let (dark, accent, index, settings) = {
        let mut locked = state.lock().unwrap();
        locked.settings.theme = if locked.dark {
            "light".to_string()
        } else {
            "dark".to_string()
        };
        locked.dark = resolve_dark(&locked.settings.theme, locked.system_dark);
        (
            locked.dark,
            locked.settings.accent.clone(),
            theme_index(&locked.settings.theme),
            locked.settings.clone(),
        )
    };
    settings::save(&settings);
    theme::apply(ui, dark, &accent);
    ui.set_theme_preference(index);
}

fn set_theme(ui: &AppWindow, state: &Arc<Mutex<UiState>>, index: i32) {
    let preference = THEMES
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or("system");
    let (dark, accent, settings) = {
        let mut locked = state.lock().unwrap();
        locked.settings.theme = preference.to_string();
        locked.dark = resolve_dark(&locked.settings.theme, locked.system_dark);
        (
            locked.dark,
            locked.settings.accent.clone(),
            locked.settings.clone(),
        )
    };
    settings::save(&settings);
    theme::apply(ui, dark, &accent);
    ui.set_theme_preference(theme_index(preference));
}

fn set_language(ui: &AppWindow, state: &Arc<Mutex<UiState>>, index: i32) {
    let preference = i18n::language_preference(index).to_string();
    let (texts, settings) = {
        let mut locked = state.lock().unwrap();
        locked.settings.language = preference.clone();
        locked.i18n.set_preference(&preference);
        (build_texts(&locked.i18n), locked.settings.clone())
    };
    settings::save(&settings);
    ui.set_texts(texts);
    ui.set_language_preference(index);
    let locked = state.lock().unwrap();
    render(ui, &locked);
}

fn set_refresh(ui: &AppWindow, state: &Arc<Mutex<UiState>>, index: i32) {
    let seconds = REFRESH_INTERVALS
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or(3);
    let settings = {
        let mut locked = state.lock().unwrap();
        locked.settings.refresh_seconds = seconds;
        locked.next_refresh = Instant::now() + Duration::from_secs(seconds);
        locked.settings.clone()
    };
    settings::save(&settings);
    ui.set_refresh_index(refresh_index(seconds));
}

fn set_accent(ui: &AppWindow, state: &Arc<Mutex<UiState>>, index: i32) {
    let accent = ACCENTS
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or("blue");
    let (dark, settings) = {
        let mut locked = state.lock().unwrap();
        locked.settings.accent = accent.to_string();
        (locked.dark, locked.settings.clone())
    };
    settings::save(&settings);
    theme::apply(ui, dark, accent);
    ui.set_accent_index(accent_index(accent));
}

fn device_to_ui(state: &UiState, device: &audio::DeviceDto) -> Device {
    let name = if device.name.is_empty() {
        state.i18n.t("device.unknown")
    } else {
        device.name.clone()
    };
    let active = state.active.as_deref() == Some(device.id.as_str());
    let selected = state.selected.as_deref() == Some(device.id.as_str());
    let favorite = state.settings.favorites.iter().any(|id| id == &device.id);
    let meta = if active {
        state.i18n.t("device.streaming")
    } else if selected {
        state.i18n.t("device.selected")
    } else if device.is_enabled {
        state.i18n.t("device.readyReceive")
    } else {
        state.i18n.t("device.unavailable")
    };
    let connect_label = state.i18n.t_args("aria.connectDevice", &[("name", &name)]);
    let disconnect_label = state
        .i18n
        .t_args("aria.disconnectDevice", &[("name", &name)]);
    let favorite_label = state.i18n.t_args(
        if favorite {
            "device.favorite-remove"
        } else {
            "device.favorite-add"
        },
        &[("name", &name)],
    );

    Device {
        id: device.id.clone().into(),
        name: name.into(),
        meta: meta.into(),
        enabled: device.is_enabled,
        active,
        selected,
        favorite,
        connect_label: connect_label.into(),
        disconnect_label: disconnect_label.into(),
        favorite_label: favorite_label.into(),
    }
}

/// The card always shows at most three devices: favorites first (in the order
/// they were starred), then the first remaining devices.
fn visible_devices<'a>(state: &'a UiState) -> Vec<&'a audio::DeviceDto> {
    let mut visible: Vec<&audio::DeviceDto> = Vec::with_capacity(3);

    for id in &state.settings.favorites {
        if visible.len() == 3 {
            break;
        }
        if let Some(device) = state.devices.iter().find(|device| &device.id == id)
            && !visible.iter().any(|existing| existing.id == device.id)
        {
            visible.push(device);
        }
    }

    for device in &state.devices {
        if visible.len() == 3 {
            break;
        }
        if !visible.iter().any(|existing| existing.id == device.id) {
            visible.push(device);
        }
    }

    visible
}

fn render(ui: &AppWindow, state: &UiState) {
    let visible: Vec<Device> = visible_devices(state)
        .into_iter()
        .map(|device| device_to_ui(state, device))
        .collect();
    let all: Vec<Device> = state
        .devices
        .iter()
        .map(|device| device_to_ui(state, device))
        .collect();

    // Only replace the Slint models when the data actually changed. Rescanning
    // every 2 seconds must not rebuild the list (and lose hover/scroll state).
    let signature = {
        let mut signature = String::new();
        for device in &visible {
            signature.push_str(&format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1e}",
                device.id,
                device.name,
                device.meta,
                device.enabled,
                device.active,
                device.selected,
                device.favorite,
                device.connect_label,
                device.disconnect_label,
                device.favorite_label,
            ));
        }
        signature.push_str("\u{1d}");
        for device in &all {
            signature.push_str(&format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1e}",
                device.id,
                device.name,
                device.meta,
                device.enabled,
                device.active,
                device.selected,
                device.favorite,
            ));
        }
        signature
    };
    if state.last_device_signature.borrow().as_str() != signature {
        *state.last_device_signature.borrow_mut() = signature;
        ui.set_devices(ModelRc::new(VecModel::from(visible)));
        ui.set_more_devices(ModelRc::new(VecModel::from(all)));
    }

    ui.set_device_count_text(state.devices.len().to_string().into());
    let show_loading = state.loading && state.devices.is_empty();
    ui.set_loading(show_loading);
    ui.set_connecting(state.connecting || state.closing);
    let connected = state.active.is_some() && !state.closing;
    ui.set_connected(connected);

    let active_name = state
        .active
        .as_ref()
        .map(|device_id| device_name(state, device_id));
    let selected_name = state
        .selected
        .as_ref()
        .map(|device_id| device_name(state, device_id));
    let fallback_name = state.i18n.t("device.unknown");

    let (title, description, is_error) = if state.closing {
        (
            state.i18n.t("status.disconnecting"),
            state.i18n.t_args(
                "status.disconnectingWithName",
                &[("name", active_name.as_deref().unwrap_or(&fallback_name))],
            ),
            false,
        )
    } else if state.connecting {
        (
            state.i18n.t("status.connecting"),
            state.i18n.t_args(
                "status.connectingWithName",
                &[("name", selected_name.as_deref().unwrap_or(&fallback_name))],
            ),
            false,
        )
    } else if let Some(error) = &state.error {
        (
            state.i18n.t("status.error"),
            localize_error(&state.i18n, error),
            true,
        )
    } else if let Some(name) = &active_name {
        (
            state.i18n.t("status.streaming"),
            state
                .i18n
                .t_args("status.streamingWithName", &[("name", name)]),
            false,
        )
    } else if show_loading {
        (
            state.i18n.t("status.ready"),
            state.i18n.t("status.scanning"),
            false,
        )
    } else if let Some(name) = &selected_name {
        (
            state.i18n.t("status.ready"),
            state.i18n.t_args("status.readyWithName", &[("name", name)]),
            false,
        )
    } else {
        (
            state.i18n.t("status.ready"),
            state.i18n.t("status.readyNoDevice"),
            false,
        )
    };

    sync_tray(state, &title);
    ui.set_status_title(title.into());
    ui.set_status_desc(description.into());
    ui.set_status_error(is_error);

    let bottom_state = if state.closing {
        state.i18n.t("bottom.disconnecting")
    } else if state.connecting {
        state.i18n.t("bottom.connecting")
    } else if is_error {
        state.i18n.t("bottom.error")
    } else if connected {
        state.i18n.t("bottom.streaming")
    } else {
        state.i18n.t("bottom.idle")
    };
    ui.set_bottom_state(bottom_state.into());

    let center_aria = if state.connecting || state.closing {
        state.i18n.t("aria.connecting")
    } else if connected {
        state.i18n.t("aria.closeConnection")
    } else {
        state.i18n.t("aria.openConnection")
    };
    ui.set_center_aria(center_aria.into());
}

fn toggle_favorite(ui: &AppWindow, state: &Arc<Mutex<UiState>>, device_id: &str) {
    let mut limit_message = None;
    {
        let mut locked = state.lock().unwrap();
        let favorites = &mut locked.settings.favorites;
        if let Some(position) = favorites.iter().position(|id| id == device_id) {
            favorites.remove(position);
        } else if favorites.len() >= 3 {
            limit_message = Some(locked.i18n.t("snackbar.favoriteLimit"));
        } else {
            favorites.push(device_id.to_string());
        }
        if limit_message.is_none() {
            settings::save(&locked.settings);
        }
    }

    if let Some(message) = limit_message {
        show_snackbar(ui, state, message);
    }
    let locked = state.lock().unwrap();
    render(ui, &locked);
}

fn sync_tray(state: &UiState, status_title: &str) {
    TRAY.with(|slot| {
        if let Some(tray) = slot.borrow().as_ref() {
            tray.set_show_label(state.i18n.t("tray.show").into());
            tray.set_disconnect_label(state.i18n.t("tray.disconnect").into());
            tray.set_quit_label(state.i18n.t("tray.quit").into());
            tray.set_can_disconnect(state.active.is_some() && !state.closing);
            tray.set_tray_tooltip(format!("Auxlane · {status_title}").into());
        }
    });
}

fn device_name(state: &UiState, device_id: &str) -> String {
    state
        .devices
        .iter()
        .find(|device| device.id == device_id)
        .map(|device| {
            if device.name.is_empty() {
                state.i18n.t("device.unknown")
            } else {
                device.name.clone()
            }
        })
        .unwrap_or_else(|| device_id.to_string())
}

fn show_snackbar(ui: &AppWindow, state: &Arc<Mutex<UiState>>, message: String) {
    let mut locked = state.lock().unwrap();
    show_snackbar_locked(ui, &mut locked, message);
}

fn show_snackbar_locked(ui: &AppWindow, state: &mut UiState, message: String) {
    state.snackbar_until = Some(Instant::now() + Duration::from_secs(6));
    ui.set_snackbar_text(message.into());
    ui.set_snackbar_visible(true);
}

fn localize_error(i18n: &I18n, error: &CommandError) -> String {
    let code = error.hresult.map(|value| format!("0x{value:08X}"));
    match error.code.as_str() {
        "unsupported" => match &code {
            Some(code) => format!("{} ({code})", i18n.t("error.unsupportedDevice")),
            None => i18n.t("error.unsupportedDevice"),
        },
        "timeout" => i18n.t("error.timeout"),
        "denied" => i18n.t("error.denied"),
        "unknownFailure" => i18n.t_args(
            "error.unknownFailure",
            &[("code", code.as_deref().unwrap_or("unknown"))],
        ),
        "notReady" => i18n.t("error.notReady"),
        "stateUnavailable" | "shuttingDown" => i18n.t("error.stateUnavailable"),
        "windows" => i18n.t_args(
            "error.windows",
            &[("code", code.as_deref().unwrap_or("unknown"))],
        ),
        "settings" => i18n.t("error.settings"),
        "deviceList" => i18n.t("error.deviceList"),
        _ => {
            if error.message.is_empty() {
                i18n.t("error.generic")
            } else {
                error.message.clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str) -> audio::DeviceDto {
        audio::DeviceDto {
            id: id.to_string(),
            name: name.to_string(),
            is_enabled: true,
        }
    }

    fn visible_ids(state: &UiState) -> Vec<String> {
        visible_devices(state)
            .into_iter()
            .map(|device| device.id.clone())
            .collect()
    }

    #[test]
    fn card_shows_first_three_by_default() {
        let mut state = UiState::new(Settings::default());
        state.devices = vec![
            device("a", "A"),
            device("b", "B"),
            device("c", "C"),
            device("d", "D"),
        ];
        assert_eq!(visible_ids(&state), vec!["a", "b", "c"]);
    }

    #[test]
    fn favorites_come_first_and_are_capped_at_three() {
        let mut state = UiState::new(Settings::default());
        state.devices = vec![
            device("a", "A"),
            device("b", "B"),
            device("c", "C"),
            device("d", "D"),
        ];

        state.settings.favorites = vec!["d".into()];
        assert_eq!(visible_ids(&state), vec!["d", "a", "b"]);

        state.settings.favorites = vec!["c".into(), "b".into(), "a".into()];
        assert_eq!(visible_ids(&state), vec!["c", "b", "a"]);
    }

    #[test]
    fn missing_favorites_are_ignored() {
        let mut state = UiState::new(Settings::default());
        state.devices = vec![device("a", "A"), device("b", "B")];
        state.settings.favorites = vec!["missing".into(), "b".into()];
        assert_eq!(visible_ids(&state), vec!["b", "a"]);
    }
}
