use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    LogicalSize, Manager, Size, WebviewWindowBuilder, WindowEvent,
};

#[cfg(windows)]
use windows_sys::Win32::{
    Graphics::Gdi::{GetDC, ReleaseDC},
    UI::ColorSystem::SetDeviceGammaRamp,
};

#[cfg(windows)]
#[repr(C)]
struct MagColorEffect {
    transform: [f32; 25],
}

#[cfg(windows)]
#[link(name = "Magnification")]
extern "system" {
    fn MagInitialize() -> i32;
    fn MagSetFullscreenTransform(mag_level: f32, x_offset: i32, y_offset: i32) -> i32;
    fn MagSetFullscreenColorEffect(effect: *const MagColorEffect) -> i32;
}

const MIN_WINDOW_WIDTH: f64 = 1120.0;
const MIN_WINDOW_HEIGHT: f64 = 820.0;
static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_profiles,
            apply_profile,
            reset_display,
            get_active_profile,
            open_data_folder,
            choose_wallpaper,
            load_wallpaper,
            import_profiles,
            export_profile,
            get_startup_settings,
            set_startup_settings
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let start_minimized = std::env::args().any(|arg| arg == "--start-minimized");

            if let Some(window) = app.get_webview_window("main") {
                let minimum_size = Size::Logical(LogicalSize {
                    width: MIN_WINDOW_WIDTH,
                    height: MIN_WINDOW_HEIGHT,
                });
                let _ = window.set_min_size(Some(minimum_size));
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
                if start_minimized {
                    let _ = window.hide();
                }
            }

            let open_item = MenuItem::with_id(app, "open", "Open Colorify", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            let app_handle = app.handle().clone();
            let mut tray_builder = TrayIconBuilder::new()
                .tooltip("Colorify")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => show_main_window(app),
                    "quit" => {
                        SHOULD_EXIT.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(&app_handle);
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            tray_builder.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.destroy();
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !SHOULD_EXIT.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let Some(config) = app.config().app.windows.first().cloned() else {
            return;
        };

        if let Ok(window) = WebviewWindowBuilder::from_config(&app, &config).and_then(|builder| builder.build()) {
            let minimum_size = Size::Logical(LogicalSize {
                width: MIN_WINDOW_WIDTH,
                height: MIN_WINDOW_HEIGHT,
            });
            let _ = window.set_min_size(Some(minimum_size));
            if let Some(icon) = app.default_window_icon() {
                let _ = window.set_icon(icon.clone());
            }
            let _ = window.set_focus();
        }
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorProfile {
    #[serde(alias = "Name")]
    name: String,
    #[serde(alias = "Brightness")]
    brightness: f64,
    #[serde(alias = "Contrast")]
    contrast: f64,
    #[serde(alias = "Saturation")]
    saturation: f64,
    #[serde(alias = "Hue")]
    hue: f64,
    #[serde(alias = "Gamma")]
    gamma: f64,
    #[serde(alias = "Temperature")]
    temperature: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileFile {
    #[serde(alias = "Profiles")]
    profiles: Vec<ColorProfile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WallpaperSelection {
    path: String,
    data_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupSettings {
    enabled: bool,
    start_minimized: bool,
}

#[derive(Debug, Default)]
struct AppState {
    profiles: Mutex<Vec<ColorProfile>>,
    active_profile: Mutex<Option<ColorProfile>>,
}

fn built_in_profiles() -> Vec<ColorProfile> {
    vec![
        profile("Custom", 0.00, 1.00, 1.00, 0.0, 1.00, 6500.0),
        profile("Default", 0.00, 1.00, 1.00, 0.0, 1.00, 6500.0),
        profile("FPS Visibility", 0.04, 1.14, 1.22, 0.0, 1.32, 7000.0),
        profile("Competitive Clarity", 0.02, 1.18, 1.32, 0.0, 1.18, 6800.0),
        profile("Black Equalizer", 0.07, 1.04, 1.12, 0.0, 1.48, 7200.0),
        profile("Digital Vibrance", 0.00, 1.08, 1.70, 0.0, 1.00, 6600.0),
        profile("True Color", 0.00, 1.02, 1.06, 0.0, 1.00, 6500.0),
        profile("Cinema Warm", 0.00, 1.06, 1.14, 0.0, 0.96, 5600.0),
    ]
}

fn profile(
    name: &str,
    brightness: f64,
    contrast: f64,
    saturation: f64,
    hue: f64,
    gamma: f64,
    temperature: f64,
) -> ColorProfile {
    ColorProfile {
        name: name.to_string(),
        brightness,
        contrast,
        saturation,
        hue,
        gamma,
        temperature,
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

#[tauri::command]
fn get_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<ColorProfile>, String> {
    let mut profiles = state
        .profiles
        .lock()
        .map_err(|_| "Profile state lock failed.".to_string())?;

    if profiles.is_empty() {
        *profiles = built_in_profiles();
    }

    Ok(profiles.clone())
}

#[tauri::command]
fn apply_profile(profile: ColorProfile, state: tauri::State<'_, AppState>) -> Result<String, String> {
    apply_display_profile(&profile)?;

    let mut active = state
        .active_profile
        .lock()
        .map_err(|_| "Profile state lock failed.".to_string())?;

    *active = Some(profile.clone());
    Ok("Current profile applied.".to_string())
}

#[tauri::command]
fn reset_display(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let default_profile = profile("Default", 0.00, 1.00, 1.00, 0.0, 1.00, 6500.0);
    apply_display_profile(&default_profile)?;

    let mut active = state
        .active_profile
        .lock()
        .map_err(|_| "Profile state lock failed.".to_string())?;

    *active = Some(default_profile);
    Ok("Reset to Default.".to_string())
}

#[tauri::command]
fn get_active_profile(state: tauri::State<'_, AppState>) -> Result<Option<ColorProfile>, String> {
    let active = state
        .active_profile
        .lock()
        .map_err(|_| "Profile state lock failed.".to_string())?;

    Ok(active.clone())
}

#[tauri::command]
fn open_data_folder() -> Result<String, String> {
    let folder = data_folder()?;
    fs::create_dir_all(&folder).map_err(|error| format!("Could not create data folder: {error}"))?;

    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(&folder)
            .spawn()
            .map_err(|error| format!("Could not open data folder: {error}"))?;
    }

    Ok(format!("Opened {}.", folder.display()))
}

#[tauri::command]
fn choose_wallpaper() -> Result<Option<WallpaperSelection>, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Choose Colorify background image")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"])
        .pick_file()
    else {
        return Ok(None);
    };

    let data_url = image_data_url(&path)?;
    Ok(Some(WallpaperSelection {
        path: path.to_string_lossy().to_string(),
        data_url,
    }))
}

#[tauri::command]
fn load_wallpaper(path: String) -> Result<String, String> {
    image_data_url(&PathBuf::from(path))
}

#[tauri::command]
fn import_profiles() -> Result<Vec<ColorProfile>, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Import Colorify profiles")
        .add_filter("Colorify profiles", &["json"])
        .pick_file()
    else {
        return Ok(Vec::new());
    };

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;

    if let Ok(profile_file) = serde_json::from_str::<ProfileFile>(&text) {
        return Ok(profile_file.profiles);
    }

    if let Ok(profiles) = serde_json::from_str::<Vec<ColorProfile>>(&text) {
        return Ok(profiles);
    }

    if let Ok(profile) = serde_json::from_str::<ColorProfile>(&text) {
        return Ok(vec![profile]);
    }

    Err("That JSON file does not look like a Colorify profile export.".to_string())
}

#[tauri::command]
fn export_profile(profile: ColorProfile) -> Result<String, String> {
    let suggested_name = format!("{}.json", safe_file_name(&profile.name));
    let Some(path) = rfd::FileDialog::new()
        .set_title("Export Colorify profile")
        .add_filter("Colorify profile", &["json"])
        .set_file_name(&suggested_name)
        .save_file()
    else {
        return Ok("Export cancelled.".to_string());
    };

    let json = serde_json::to_string_pretty(&profile)
        .map_err(|error| format!("Could not serialize profile: {error}"))?;
    fs::write(&path, json).map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(format!("Exported {}.", path.display()))
}

#[tauri::command]
fn get_startup_settings() -> Result<StartupSettings, String> {
    let shortcut = startup_shortcut_path()?;

    if !shortcut.exists() {
        return Ok(StartupSettings {
            enabled: false,
            start_minimized: false,
        });
    }

    Ok(StartupSettings {
        enabled: true,
        start_minimized: shortcut_arguments(&shortcut)?.contains("--start-minimized"),
    })
}

#[tauri::command]
fn set_startup_settings(enabled: bool, start_minimized: bool) -> Result<String, String> {
    let shortcut = startup_shortcut_path()?;

    if !enabled {
        if shortcut.exists() {
            fs::remove_file(&shortcut)
                .map_err(|error| format!("Could not remove startup shortcut: {error}"))?;
        }
        return Ok("Start with Windows disabled.".to_string());
    }

    let Some(parent) = shortcut.parent() else {
        return Err("Could not resolve Startup folder.".to_string());
    };
    fs::create_dir_all(parent).map_err(|error| format!("Could not create Startup folder: {error}"))?;

    let exe = std::env::current_exe().map_err(|error| format!("Could not find Colorify executable: {error}"))?;
    let working_dir = exe
        .parent()
        .ok_or("Could not resolve Colorify install folder.".to_string())?;
    let arguments = if start_minimized { "--start-minimized" } else { "" };

    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell; \
         $shortcut = $shell.CreateShortcut({}); \
         $shortcut.TargetPath = {}; \
         $shortcut.Arguments = {}; \
         $shortcut.WorkingDirectory = {}; \
         $shortcut.IconLocation = {}; \
         $shortcut.Save()",
        powershell_string(&shortcut),
        powershell_string(&exe),
        powershell_string(arguments),
        powershell_string(working_dir),
        powershell_string(&exe),
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|error| format!("Could not create startup shortcut: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    if start_minimized {
        Ok("Start with Windows enabled. Colorify will start minimized.".to_string())
    } else {
        Ok("Start with Windows enabled.".to_string())
    }
}

fn data_folder() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not available.".to_string())?;
    Ok(PathBuf::from(appdata).join("Colorify"))
}

fn startup_shortcut_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not available.".to_string())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("Colorify.lnk"))
}

fn shortcut_arguments(shortcut: &PathBuf) -> Result<String, String> {
    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell; \
         $shortcut = $shell.CreateShortcut({}); \
         Write-Output $shortcut.Arguments",
        powershell_string(shortcut),
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|error| format!("Could not inspect startup shortcut: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn powershell_string(value: impl AsRef<std::ffi::OsStr>) -> String {
    let text = value.as_ref().to_string_lossy().replace('\'', "''");
    format!("'{text}'")
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                character
            }
        })
        .collect();

    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Colorify profile".to_string()
    } else {
        trimmed.to_string()
    }
}

fn image_data_url(path: &PathBuf) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return Err("Choose a PNG, JPG, WEBP, or BMP image.".to_string()),
    };

    let bytes = fs::read(path).map_err(|error| format!("Could not read image: {error}"))?;
    let encoded = general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn apply_display_profile(profile: &ColorProfile) -> Result<(), String> {
    #[cfg(windows)]
    {
        let hardware_gamma_applied = set_gamma(profile.gamma).is_ok();
        set_color_effect(&color_matrix_from_profile(profile, !hardware_gamma_applied))
    }

    #[cfg(not(windows))]
    {
        let _ = profile;
        Err("Display calibration is only implemented on Windows.".to_string())
    }
}

fn temperature_scale(temperature: f64) -> (f64, f64, f64) {
    if (temperature - 6500.0).abs() < 50.0 {
        return (1.0, 1.0, 1.0);
    }

    let temp = temperature / 100.0;
    let (mut red, mut green, mut blue);

    if temp <= 66.0 {
        red = 255.0;
        green = (99.4708025861 * temp.ln()) - 161.1195681661;
        blue = if temp <= 19.0 {
            0.0
        } else {
            (138.5177312231 * (temp - 10.0).ln()) - 305.0447927307
        };
    } else {
        red = 329.698727446 * (temp - 60.0).powf(-0.1332047592);
        green = 288.1221695283 * (temp - 60.0).powf(-0.0755148492);
        blue = 255.0;
    }

    red = clamp(red, 0.0, 255.0);
    green = clamp(green, 0.0, 255.0);
    blue = clamp(blue, 0.0, 255.0);

    let max = red.max(green).max(blue);
    if max <= 0.0 {
        (1.0, 1.0, 1.0)
    } else {
        (red / max, green / max, blue / max)
    }
}

fn identity_matrix() -> [f64; 25] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn multiply_matrix(left: [f64; 25], right: [f64; 25]) -> [f64; 25] {
    let mut result = [0.0; 25];
    for row in 0..5 {
        for col in 0..5 {
            let mut sum = 0.0;
            for k in 0..5 {
                sum += left[(row * 5) + k] * right[(k * 5) + col];
            }
            result[(row * 5) + col] = sum;
        }
    }
    result
}

fn brightness_matrix(brightness: f64) -> [f64; 25] {
    let mut matrix = identity_matrix();
    matrix[20] = brightness;
    matrix[21] = brightness;
    matrix[22] = brightness;
    matrix
}

fn contrast_matrix(contrast: f64) -> [f64; 25] {
    let offset = 0.5 * (1.0 - contrast);
    let mut matrix = identity_matrix();
    matrix[0] = contrast;
    matrix[6] = contrast;
    matrix[12] = contrast;
    matrix[20] = offset;
    matrix[21] = offset;
    matrix[22] = offset;
    matrix
}

fn saturation_matrix(saturation: f64) -> [f64; 25] {
    let rw = 0.213;
    let gw = 0.715;
    let bw = 0.072;
    let inverse = 1.0 - saturation;

    [
        (rw * inverse) + saturation, rw * inverse, rw * inverse, 0.0, 0.0,
        gw * inverse, (gw * inverse) + saturation, gw * inverse, 0.0, 0.0,
        bw * inverse, bw * inverse, (bw * inverse) + saturation, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn hue_matrix(degrees: f64) -> [f64; 25] {
    let angle = degrees.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();

    let a00 = 0.213 + (cos * 0.787) - (sin * 0.213);
    let a01 = 0.715 - (cos * 0.715) - (sin * 0.715);
    let a02 = 0.072 - (cos * 0.072) + (sin * 0.928);
    let a10 = 0.213 - (cos * 0.213) + (sin * 0.143);
    let a11 = 0.715 + (cos * 0.285) + (sin * 0.140);
    let a12 = 0.072 - (cos * 0.072) - (sin * 0.283);
    let a20 = 0.213 - (cos * 0.213) - (sin * 0.787);
    let a21 = 0.715 - (cos * 0.715) + (sin * 0.715);
    let a22 = 0.072 + (cos * 0.928) + (sin * 0.072);

    [
        a00, a10, a20, 0.0, 0.0,
        a01, a11, a21, 0.0, 0.0,
        a02, a12, a22, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn temperature_matrix(temperature: f64) -> [f64; 25] {
    let (red, green, blue) = temperature_scale(temperature);
    let mut matrix = identity_matrix();
    matrix[0] = red;
    matrix[6] = green;
    matrix[12] = blue;
    matrix
}

fn gamma_compatibility_matrix(gamma: f64) -> [f64; 25] {
    let gamma = clamp(gamma, 0.5, 2.5);
    if (gamma - 1.0).abs() < 0.01 {
        return identity_matrix();
    }

    // The Magnification API only accepts a linear color matrix, so this is an
    // approximation used when the display driver rejects SetDeviceGammaRamp.
    let normalized = clamp(gamma.ln() / 2.5_f64.ln(), -1.0, 1.0);
    let contrast = if normalized >= 0.0 {
        1.0 - (normalized * 0.30)
    } else {
        1.0 - (normalized * 0.28)
    };
    let brightness = if normalized >= 0.0 {
        normalized * 0.16
    } else {
        normalized * 0.10
    };

    multiply_matrix(contrast_matrix(contrast), brightness_matrix(brightness))
}

fn color_matrix_from_profile(profile: &ColorProfile, include_gamma_compatibility: bool) -> [f32; 25] {
    let mut matrix = identity_matrix();
    matrix = multiply_matrix(matrix, temperature_matrix(clamp(profile.temperature, 3000.0, 10000.0)));
    matrix = multiply_matrix(matrix, hue_matrix(clamp(profile.hue, -180.0, 180.0)));
    matrix = multiply_matrix(matrix, saturation_matrix(clamp(profile.saturation, 0.0, 5.0)));
    if include_gamma_compatibility {
        matrix = multiply_matrix(matrix, gamma_compatibility_matrix(profile.gamma));
    }
    matrix = multiply_matrix(matrix, contrast_matrix(clamp(profile.contrast, 0.5, 2.0)));
    matrix = multiply_matrix(matrix, brightness_matrix(clamp(profile.brightness, -0.25, 0.25)));

    let mut result = [0.0_f32; 25];
    for i in 0..25 {
        result[i] = matrix[i] as f32;
    }
    result
}

#[cfg(windows)]
fn set_color_effect(matrix: &[f32; 25]) -> Result<(), String> {
    unsafe {
        if MagInitialize() == 0 {
            return Err("Windows Magnification API could not start.".to_string());
        }

        MagSetFullscreenTransform(1.0, 0, 0);
        let effect = MagColorEffect { transform: *matrix };
        if MagSetFullscreenColorEffect(&effect) == 0 {
            return Err("Windows rejected the color matrix.".to_string());
        }
    }

    Ok(())
}

#[cfg(windows)]
fn set_gamma(gamma: f64) -> Result<(), String> {
    #[cfg(windows)]
    {
        let gamma = clamp(gamma, 0.35, 3.0);
        let mut ramp = [[0u16; 256]; 3];
        for i in 0..256 {
            let input = i as f64 / 255.0;
            let mapped = clamp(input.powf(1.0 / gamma), 0.0, 1.0);
            let value = (mapped * 65535.0).round() as u16;
            ramp[0][i] = value;
            ramp[1][i] = value;
            ramp[2][i] = value;
        }

        unsafe {
            let hdc = GetDC(std::ptr::null_mut());
            if hdc.is_null() {
                return Err("Could not access the desktop display context.".to_string());
            }

            let ok = SetDeviceGammaRamp(hdc, ramp.as_mut_ptr() as *mut _);
            ReleaseDC(std::ptr::null_mut(), hdc);

            if ok == 0 {
                return Err("Display driver rejected the calibration ramp.".to_string());
            }
        }

        Ok(())
    }
}
