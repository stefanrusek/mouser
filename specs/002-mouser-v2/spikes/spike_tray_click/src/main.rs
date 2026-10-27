//! Probe P4 (T015): tray click events and runtime set_icon. Observes only: injects no input,
//! grabs nothing, never touches the clipboard. The owner clicks the tray icon by hand.
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::RunEvent;

struct Log {
    file: File,
    start: Instant,
}

impl Log {
    fn line(&mut self, text: &str) {
        let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let line = format!("[wall_ms={wall} +{:.3}s] {text}", self.start.elapsed().as_secs_f64());
        println!("{line}");
        writeln!(self.file, "{line}").unwrap();
        self.file.flush().unwrap();
    }
}

const RUNBOOK: &str = "\
spike_tray_click (probe P4). Observes only; no input is injected.
1. Find the tray icon (macOS menu bar / Windows notification area; may be under the ^ overflow).
2. Left-click it 3 times, slowly, about 1 second apart. Each Up should swap the badge.
3. Right-click it 2 times (menu opens; press Escape or click away to close it).
4. Open the menu once and choose 'Menu item (log only)'.
5. Open the menu and choose 'Quit spike'.
6. Send back spike_tray_click.log (path printed below). See RUNBOOK.md for the criteria.
";

fn main() {
    let exe = std::env::current_exe().unwrap();
    let path = exe.parent().unwrap().join("spike_tray_click.log");
    let file = OpenOptions::new().create(true).append(true).open(&path).unwrap();
    let log = Arc::new(Mutex::new(Log { file, start: Instant::now() }));
    print!("{RUNBOOK}");
    log.lock().unwrap().line(&format!(
        "start os={} arch={} log={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        path.display()
    ));

    let icon_a = Image::from_bytes(include_bytes!("../icons/badge_a.png")).unwrap();
    let icon_b = Image::from_bytes(include_bytes!("../icons/badge_b.png")).unwrap();
    let icons = [icon_a.clone(), icon_b];
    let which = Arc::new(Mutex::new(0usize));

    let log_menu = log.clone();
    let log_tray = log.clone();
    let log_run = log.clone();
    let which_tray = which.clone();

    let app = tauri::Builder::default()
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref().to_string();
            log_menu.lock().unwrap().line(&format!("MenuEvent id={id}"));
            if id == "quit" {
                app.exit(0);
            }
        })
        .setup(move |app| {
            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "item", "Menu item (log only)", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "Quit spike", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::with_id("spike")
                .icon(icon_a)
                .tooltip("spike_tray_click")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(move |tray, event| {
                    let mut l = log_tray.lock().unwrap();
                    match &event {
                        TrayIconEvent::Click { button, button_state, position, .. } => {
                            l.line(&format!(
                                "TrayIconEvent::Click button={button:?} button_state={button_state:?} position=({:.1},{:.1})",
                                position.x, position.y
                            ));
                            if *button == MouseButton::Left && *button_state == MouseButtonState::Up {
                                let mut w = which_tray.lock().unwrap();
                                *w = 1 - *w;
                                let r = tray.set_icon(Some(icons[*w].clone()));
                                l.line(&format!("set_icon variant={} result={r:?}", if *w == 0 { "A" } else { "B" }));
                            }
                        }
                        TrayIconEvent::DoubleClick { button, position, .. } => l.line(&format!(
                            "TrayIconEvent::DoubleClick button={button:?} position=({:.1},{:.1})",
                            position.x, position.y
                        )),
                        TrayIconEvent::Enter { position, .. } => {
                            l.line(&format!("TrayIconEvent::Enter position=({:.1},{:.1})", position.x, position.y))
                        }
                        TrayIconEvent::Move { position, .. } => {
                            l.line(&format!("TrayIconEvent::Move position=({:.1},{:.1})", position.x, position.y))
                        }
                        TrayIconEvent::Leave { position, .. } => {
                            l.line(&format!("TrayIconEvent::Leave position=({:.1},{:.1})", position.x, position.y))
                        }
                        other => l.line(&format!("TrayIconEvent::other {other:?}")),
                    }
                })
                .build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .unwrap();

    app.run(move |_app, event| {
        if let RunEvent::ExitRequested { code, api, .. } = &event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        if let RunEvent::Exit = event {
            log_run.lock().unwrap().line("exit");
        }
    });
}
