use tray_icon::{
    menu::{ MenuEvent},
    TrayEvent, TrayIconBuilder, ClickEvent,
};

use tray_icon::menu::Menu;
use winit::event_loop::{EventLoopBuilder};
use winit::platform::windows::EventLoopBuilderExtWindows;
use std::thread::{self};
use std::time::Duration;
use std::process::Command;

use super::progress_ui::ProgressUI;
pub struct SystemTray;

impl SystemTray {
    fn load_logic() {
        let icon = SystemTray::load_icon();
        let mut  builder = EventLoopBuilder::new();
        builder.with_any_thread(true);
        let event_loop = builder.build();
        let _tray =match TrayIconBuilder::new()
        .with_tooltip("Phantom Agent").with_menu(Box::new(Menu::new()))
        .with_icon(icon)
        .build() {
            Ok(tray) => { Ok(tray) },
            Err(err) => {
                let error_message = format!("Failed building tray icon: {err}");
                println!("{}", error_message);
                Err(error_message)
            }
        };

       let _menu_channel = MenuEvent::receiver();
       let tray_channel = TrayEvent::receiver();

       event_loop.run(move |_event, _, control_flow| {
           control_flow.set_wait_until(std::time::Instant::now() + Duration::from_millis(50));
           if let Ok(event) = tray_channel.try_recv() {
               if event.event == ClickEvent::Right {
                   Command::new("powershell")
                       .arg("Get-Content").arg("\"C:\\Program Files\\phantom_agent\\log\\phantom_agent.log\"").arg("-Wait").arg("-Tail").arg("30")
                       .spawn()
                       .expect("ls command failed to start");
               } else if event.event == ClickEvent::Left {
                    ProgressUI::show();
               }
           }
       });
    }

    pub fn load() -> std::thread::JoinHandle<()>{
        thread::spawn(Self::load_logic)
    }

    fn load_icon() -> tray_icon::icon::Icon {
        let (icon_rgba, icon_width, icon_height) = {
            let icon = include_bytes!("phantom_logo.png");
            let image = image::load_from_memory(icon)
                .expect("Failed to open icon path")
                .into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            (rgba, width, height)
        };
        tray_icon::icon::Icon::from_rgba(icon_rgba, icon_width, icon_height)
            .expect("Failed to open icon")
    }
}