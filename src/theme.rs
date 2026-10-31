use slint::{Color, ComponentHandle};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub fn system_dark() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|light| light == 0)
        .unwrap_or(false)
}

fn hex(value: &str) -> Color {
    let value = value.trim_start_matches('#');
    let r = u8::from_str_radix(&value[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&value[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&value[4..6], 16).unwrap_or(0);
    Color::from_rgb_u8(r, g, b)
}

fn alpha(alpha: u8, red: u8, green: u8, blue: u8) -> Color {
    Color::from_argb_u8(alpha, red, green, blue)
}

fn accent_colors(dark: bool, accent: &str) -> (Color, Color, Color, Color) {
    let values = match (dark, accent) {
        (true, "teal") => ("#81e4da", "#052e2a", "#21635d", "#cbf6f2"),
        (true, "purple") => ("#cbaaf8", "#251041", "#4d356e", "#e8d8fd"),
        (true, "amber") => ("#ffd470", "#372806", "#6f5720", "#ffedc2"),
        (true, _) => ("#a7c6fb", "#082049", "#2d4876", "#cfdffc"),
        (false, "teal") => ("#178277", "#ffffff", "#c6f1ec", "#0b3d38"),
        (false, "purple") => ("#6b35b6", "#ffffff", "#e8dcf9", "#291249"),
        (false, "amber") => ("#a56f12", "#ffffff", "#ffeab8", "#402b07"),
        (false, _) => ("#1855bf", "#ffffff", "#dae6fb", "#092453"),
    };
    (hex(values.0), hex(values.1), hex(values.2), hex(values.3))
}

pub fn apply(ui: &crate::AppWindow, dark: bool, accent: &str) {
    let theme = ui.global::<crate::Theme>();
    theme.set_dark(dark);

    let (primary, on_primary, primary_container, on_primary_container) =
        accent_colors(dark, accent);
    theme.set_primary(primary);
    theme.set_on_primary(on_primary);
    theme.set_primary_container(primary_container);
    theme.set_on_primary_container(on_primary_container);

    if dark {
        theme.set_background(hex("#191e24"));
        theme.set_surface(hex("#222830"));
        theme.set_surface_container(hex("#191e24"));
        theme.set_surface_variant(hex("#3f4650"));
        theme.set_on_surface(hex("#e7eaef"));
        theme.set_on_surface_variant(hex("#a9b1bc"));
        theme.set_outline(hex("#818a98"));
        theme.set_border(hex("#38414c"));
        theme.set_secondary_container(hex("#3f4650"));
        theme.set_on_secondary_container(hex("#e7eaef"));
        theme.set_tertiary(hex("#7cded5"));
        theme.set_on_tertiary(hex("#052e2a"));
        theme.set_tertiary_container(hex("#124944"));
        theme.set_on_tertiary_container(hex("#bef4ee"));
        theme.set_error(hex("#ffb6ad"));
        theme.set_on_error(hex("#5c0900"));
        theme.set_error_container(hex("#72251d"));
        theme.set_on_error_container(hex("#ffd1cc"));
        theme.set_hairline(alpha(15, 255, 255, 255));
        theme.set_panel_border(alpha(26, 255, 255, 255));
        theme.set_shadow(alpha(90, 0, 0, 0));
    } else {
        theme.set_background(hex("#e6eaf0"));
        theme.set_surface(hex("#fcfcfd"));
        theme.set_surface_container(hex("#e6eaf0"));
        theme.set_surface_variant(hex("#d3d8de"));
        theme.set_on_surface(hex("#252c37"));
        theme.set_on_surface_variant(hex("#5e6978"));
        theme.set_outline(hex("#788391"));
        theme.set_border(hex("#cad0d8"));
        theme.set_secondary_container(hex("#dfe4eb"));
        theme.set_on_secondary_container(hex("#252c37"));
        theme.set_tertiary(hex("#178277"));
        theme.set_on_tertiary(hex("#ffffff"));
        theme.set_tertiary_container(hex("#c6f1ec"));
        theme.set_on_tertiary_container(hex("#0b3d38"));
        theme.set_error(hex("#c33222"));
        theme.set_on_error(hex("#ffffff"));
        theme.set_error_container(hex("#ffdad6"));
        theme.set_on_error_container(hex("#4e140e"));
        theme.set_hairline(alpha(15, 0, 0, 0));
        theme.set_panel_border(alpha(20, 0, 0, 0));
        theme.set_shadow(alpha(30, 0, 0, 0));
    }
}
