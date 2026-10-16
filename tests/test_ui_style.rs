use audion::ui::aui_file::{load_theme, load_widget_config, save_layout, WidgetLayout};
use audion::ui::{WidgetConfig, WidgetKind, WidgetStyle};
use std::collections::HashMap;

#[test]
fn test_style_set_keys() {
    let mut s = WidgetStyle::default();
    s.set_key("text_color", &[10.0, 20.0, 30.0]).unwrap();
    s.set_key("font_size", &[18.0]).unwrap();
    s.set_key("rounding", &[6.0]).unwrap();
    s.set_key("border_color", &[1.0, 2.0, 3.0]).unwrap();
    s.set_key("border_width", &[2.0]).unwrap();
    s.set_key("hover_color", &[300.0, -5.0, 7.0]).unwrap(); // clamped to 0-255
    assert_eq!(s.text_color, Some([10, 20, 30]));
    assert_eq!(s.font_size, Some(18.0));
    assert_eq!(s.rounding, Some(6.0));
    assert_eq!(s.border_color, Some([1, 2, 3]));
    assert_eq!(s.border_width, Some(2.0));
    assert_eq!(s.hover_color, Some([255, 0, 7]));
    s.set_key("fill_color", &[5.0, 6.0, 7.0]).unwrap();
    s.set_key("mono", &[1.0]).unwrap();
    assert_eq!(s.fill_color, Some([5, 6, 7]));
    assert_eq!(s.mono, Some(true));
}

#[test]
fn test_style_unknown_key_is_an_error() {
    let err = WidgetStyle::default().set_key("text_colour", &[1.0]).unwrap_err();
    assert!(err.contains("text_colour") && err.contains("text_color"), "{}", err);
}

#[test]
fn test_style_widget_wins_over_theme() {
    let mut theme = WidgetStyle::default();
    theme.set_key("rounding", &[8.0]).unwrap();
    theme.set_key("color", &[1.0, 1.0, 1.0]).unwrap();
    let mut widget = WidgetStyle::default();
    widget.set_key("color", &[9.0, 9.0, 9.0]).unwrap();
    let s = widget.with_fallback(&theme);
    assert_eq!(s.color, Some([9, 9, 9]), "widget's own key wins");
    assert_eq!(s.rounding, Some(8.0), "unset key comes from the theme");
}

fn tmp_aui(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("audion_test_{}_{}.aui", name, std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn test_aui_theme_and_widget_keys() {
    let path = tmp_aui("keys", r#"
[__theme]
rounding = 4
text_color = [200, 200, 210]
font_size = 13

[play]
border_color = [90, 90, 120]
border_width = 2
hover_color = [60, 60, 80]
width = 150
"#);
    let theme = load_theme(&path);
    let cfg = load_widget_config(&path, "play", WidgetConfig::new(WidgetKind::Button));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(theme.rounding, Some(4.0));
    assert_eq!(theme.text_color, Some([200, 200, 210]));
    assert_eq!(theme.font_size, Some(13.0));
    assert_eq!(cfg.style.border_color, Some([90, 90, 120]));
    assert_eq!(cfg.style.border_width, Some(2.0));
    assert_eq!(cfg.style.hover_color, Some([60, 60, 80]));
    assert_eq!(cfg.style.width, Some(150.0));
}

#[test]
fn test_aui_edit_mode_save_keeps_style_keys() {
    let path = tmp_aui("save", r#"
[__theme]
rounding = 4
hover_color = [60, 60, 80]
fill_color = [20, 22, 26]
mono = true

[play]
font_size = 20
border_width = 3
default = 0.5
x = 10
y = 10
"#);
    // edit mode moves the widget and rewrites the file
    let mut layouts = HashMap::new();
    layouts.insert("play".to_string(), WidgetLayout { x: 50.0, y: 60.0, width: None, height: None });
    save_layout(&path, &layouts, None);
    let theme = load_theme(&path);
    let cfg = load_widget_config(&path, "play", WidgetConfig::new(WidgetKind::Button));
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(theme.rounding, Some(4.0), "theme kept:\n{}", text);
    assert_eq!(theme.hover_color, Some([60, 60, 80]), "theme kept:\n{}", text);
    assert_eq!(theme.fill_color, Some([20, 22, 26]), "theme kept:\n{}", text);
    assert_eq!(theme.mono, Some(true), "theme kept:\n{}", text);
    assert_eq!(cfg.style.font_size, Some(20.0), "widget style kept:\n{}", text);
    assert_eq!(cfg.style.border_width, Some(3.0));
    assert_eq!((cfg.x, cfg.y), (Some(50.0), Some(60.0)), "new position saved");
    assert!(text.contains("default = 0.5"), "default kept:\n{}", text);
}
