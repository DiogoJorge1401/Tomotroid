/*#![warn(
    clippy::all,
    clippy::pedantic,
    //clippy::cargo,
)]*/

#[cfg(unix)]
use std::io::Cursor;

use anyhow::Result;
use hex_color::HexColor;
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{Color, ModelRc, VecModel};
use std::{
    fs::File,
    io::{BufReader, Read},
    rc::Rc,
    sync::mpsc,
};
use tray_item::{IconSource, TrayItem};
use walkdir::WalkDir;

slint::include_modules!();

pub const LOGO_BYTES: &str = include_str!("../assets/logo.svg");

//Right now serde support in Slint is new and crude, some of the types in the Slint version
//of this struct like Brush don't support serde yet. So for now I'm creating 2 versions
//the slint version and this version to manually convert between them.
#[derive(Deserialize)]
struct ThemeColors {
    #[serde(rename = "--color-long-round")]
    long_round: HexColor,

    #[serde(rename = "--color-short-round")]
    short_round: HexColor,

    #[serde(rename = "--color-focus-round")]
    focus_round: HexColor,

    #[serde(rename = "--color-background")]
    background: HexColor,

    #[serde(rename = "--color-background-light")]
    background_light: HexColor,

    #[serde(rename = "--color-background-lightest")]
    background_lightest: HexColor,

    #[serde(rename = "--color-foreground")]
    foreground: HexColor,

    #[serde(rename = "--color-foreground-darker")]
    foreground_darker: HexColor,

    #[serde(rename = "--color-foreground-darkest")]
    foreground_darkest: HexColor,

    #[serde(rename = "--color-accent")]
    accent: HexColor,
}

#[derive(Deserialize)]
struct JsonThemeTemp {
    name: String,
    colors: ThemeColors,
}

//I realize implemeting From is more idomatic, but that would require creating a newtype for JsonThemeTemp,
//due to the orphan rule, and then having to convert that (or maybe deref) that into JsonThemeTemp. I think this is a
//good and straight forward stop gap, until slint adds support for Serde to more types
impl Into<JsonTheme> for JsonThemeTemp {
    fn into(self) -> JsonTheme {
        JsonTheme {
            name: self.name.into(),
            long_round: Color::from_rgb_u8(
                self.colors.long_round.r,
                self.colors.long_round.g,
                self.colors.long_round.b,
            )
            .into(),
            short_round: Color::from_rgb_u8(
                self.colors.short_round.r,
                self.colors.short_round.g,
                self.colors.short_round.b,
            )
            .into(),
            focus_round: Color::from_rgb_u8(
                self.colors.focus_round.r,
                self.colors.focus_round.g,
                self.colors.focus_round.b,
            )
            .into(),
            background: Color::from_rgb_u8(
                self.colors.background.r,
                self.colors.background.g,
                self.colors.background.b,
            )
            .into(),
            background_light: Color::from_rgb_u8(
                self.colors.background_light.r,
                self.colors.background_light.g,
                self.colors.background_light.b,
            )
            .into(),
            background_lightest: Color::from_rgb_u8(
                self.colors.background_lightest.r,
                self.colors.background_lightest.g,
                self.colors.background_lightest.b,
            )
            .into(),
            foreground: Color::from_rgb_u8(
                self.colors.foreground.r,
                self.colors.foreground.g,
                self.colors.foreground.b,
            )
            .into(),
            foreground_darker: Color::from_rgb_u8(
                self.colors.foreground_darker.r,
                self.colors.foreground_darker.g,
                self.colors.foreground_darker.b,
            )
            .into(),
            foreground_darkest: Color::from_rgb_u8(
                self.colors.foreground_darkest.r,
                self.colors.foreground_darkest.g,
                self.colors.foreground_darkest.b,
            )
            .into(),
            accent: Color::from_rgb_u8(
                self.colors.accent.r,
                self.colors.accent.g,
                self.colors.accent.b,
            )
            .into(),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub always_on_top: bool,
    pub auto_start_break_timer: bool,
    pub auto_start_work_timer: bool,
    pub break_always_on_top: bool,
    pub global_shortcuts: GlobalShortcuts,
    pub min_to_tray: bool,
    pub min_to_tray_on_close: bool,
    pub notifications: bool,
    pub theme: String,
    pub tick_sounds: bool,
    pub tick_sounds_during_break: bool,
    pub time_long_break: i64,
    pub time_short_break: i64,
    pub time_work: i64,
    pub volume: i64,
    pub work_rounds: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalShortcuts {
    #[serde(rename = "call-timer-reset")]
    pub call_timer_reset: String,
    #[serde(rename = "call-timer-skip")]
    pub call_timer_skip: String,
    #[serde(rename = "call-timer-toggle")]
    pub call_timer_toggle: String,
}

enum TrayMsg {
    MinRes,
    Quit,
}

fn color_to_hex_string(color: slint::Color) -> String {
    format!("#{:02X}{:02X}{:02X}",color.red(), color.green(), color.blue())
}

fn main() -> Result<()> {
    //TODO: I'm not seeing an obvious way to mimic the Pomotroid behavoir
    //where it just minimizes or restores by clicking the tray icon
    //because I don't see any way to capture when the tray icon is clicked
    //I'll need to dig into this more. For now I'll just add some menu items
    //to get some basic functionality and test minimzing to the tray etc
    #[cfg(unix)]
    let mut tray = {
        let logo_cursor = Cursor::new(include_bytes!("../assets/icons/logo.png"));
        let logo_decoder = png::Decoder::new(logo_cursor);
        let mut logo_reader = logo_decoder.read_info().unwrap();
        let mut logo_buff = vec![0; logo_reader.output_buffer_size()];
        logo_reader.next_frame(&mut logo_buff).unwrap();

        let logo_icon = IconSource::Data {
            data: logo_buff,
            height: 32,
            width: 32,
        };
        TrayItem::new("Tomotroid\nClick to Restore", logo_icon).unwrap()
    };

    #[cfg(windows)]
    let mut tray = TrayItem::new(
        "Tomotroid\nClick to Restore",
        IconSource::Resource("logo-icon"),
    )
    .unwrap();

    let (tray_tx, tray_rx) = mpsc::sync_channel(1);

    let minres_tx = tray_tx.clone();
    tray.add_menu_item("Minimize / Restore", move || {
        minres_tx.send(TrayMsg::MinRes).unwrap();
    })
    .unwrap();

    let quit_tx = tray_tx; //.clone();
    tray.add_menu_item("Quit", move || {
        quit_tx.send(TrayMsg::Quit).unwrap();
    })
    .unwrap();

    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new())).unwrap();

    let main = Main::new()?;

    let close_handle = main.as_weak();
    main.on_close_window(move || {
        close_handle.upgrade().unwrap().hide().unwrap();

        //After I get the system tray working I'm going to want to hide the window instead of actually close it
        //i_slint_backend_winit::WinitWindowAccessor::with_winit_window(min_handle.window(), |win| win.set_visible(false));
    });

    let min_handle = main.as_weak();
    main.on_minimize_window(move || {
        let min_handle = min_handle.upgrade().unwrap();
        i_slint_backend_winit::WinitWindowAccessor::with_winit_window(min_handle.window(), |win| {
            win.set_minimized(true);
        });
    });

    let move_handle = main.as_weak();
    main.on_move_window(move || {
        let move_handle = move_handle.upgrade().unwrap();
        i_slint_backend_winit::WinitWindowAccessor::with_winit_window(
            move_handle.window(),
            |win| win.drag_window(),
        );
    });

    let tray_handle = main.as_weak();
    let _tray_rec_thread = std::thread::spawn(move || loop {
        match tray_rx.recv() {
            Ok(TrayMsg::MinRes) => {
                let tray_handle_copy = tray_handle.clone();
                slint::invoke_from_event_loop(move || {
                    let main = tray_handle_copy.upgrade().unwrap();
                    i_slint_backend_winit::WinitWindowAccessor::with_winit_window(
                        main.window(),
                        |win| {
                            win.set_minimized(!win.is_minimized().unwrap());
                            win.focus_window();
                        },
                    );
                })
                .unwrap();
            }
            Ok(TrayMsg::Quit) => {
                let tray_handle_copy = tray_handle.clone();
                slint::invoke_from_event_loop(move || {
                    tray_handle_copy.upgrade().unwrap().hide().unwrap();
                })
                .unwrap();
            }
            _ => {}
        }
    });

    main.global::<HLClick>().on_hl_clicked(|url| {
        open::that(url.as_str()).unwrap();
    });

    let thm_handle = main.as_weak();
    main.global::<ThemeCallbacks>()
        .on_theme_changed(move |theme| {
            let thm_handle = thm_handle.upgrade().unwrap();
            thm_handle.set_logo(
                slint::Image::load_from_svg_data(
                    LOGO_BYTES
                        .replace(
                            "stroke:#2f384b",
                            &format!("stroke:{}", color_to_hex_string(theme.background.color()))
                        )
                        .replace(
                            "fill:#ff4e4d",
                            &format!("fill:{}", color_to_hex_string(theme.focus_round.color()))
                        )
                        .replace(
                            "fill:#992e2e",
                            &format!("fill:{}", color_to_hex_string(theme.focus_round.color().darker(0.4)))
                        )
                        .replace(
                            "fill:#f6f2eb",
                            &format!("fill:{}", color_to_hex_string(theme.foreground.color()))
                        )
                        .replace(
                            "fill:#05ec8c",
                            &format!("fill:{}", color_to_hex_string(theme.accent.color()))
                        )
                        .as_bytes(),
                )
                .unwrap(),
            );
        });

    main.global::<ThemeCallbacks>().on_load_themes(move || {
        let mut theme_dir = std::env::current_dir().unwrap();
        theme_dir.push("themes");
        let themes: Vec<JsonTheme> = {
            let mut themes: Vec<JsonTheme> = WalkDir::new(theme_dir)
                .into_iter()
                .filter(|e| {
                    return e.as_ref().map_or(false, |f| {
                        f.file_name()
                            .to_str()
                            .map(|s| s.to_lowercase().ends_with(".json"))
                            .unwrap_or(false)
                    });
                })
                .filter_map(|e| {
                    e.map(|e| {
                        let reader = BufReader::new(File::open(e.path()).unwrap());
                        let theme = std::io::read_to_string(reader).unwrap();
                        let theme = serde_json::from_str::<JsonThemeTemp>(&theme)
                            .unwrap()
                            .into();
                        theme
                    })
                    .ok()
                })
                .collect();
            themes.sort_by(|a, b| a.name.partial_cmp(&b.name).unwrap());
            themes
        };

        let model: Rc<VecModel<JsonTheme>> = Rc::new(VecModel::from(themes));

        ModelRc::from(model.clone())
    });

    main.global::<ThemeCallbacks>().invoke_load_themes();
    main.run()?;
    Ok(())
}
