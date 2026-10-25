//! X11 startup on virtual desktops and driver-independent graphics error feedback.

use std::{env, path::PathBuf};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{self, ConnectionExt as _},
    },
    wrapper::ConnectionExt as _,
};

/// A virtual X11 server without DRI3 cannot present a hardware Vulkan surface.
/// Select software before entering native driver code in that known case.
pub fn retry_for_virtual_display() -> Option<std::io::Result<std::process::ExitStatus>> {
    let (connection, _) = x11rb::connect(None).ok()?;
    let dri3 = connection.query_extension(b"DRI3").ok()?.reply().ok()?;
    if dri3.present {
        return None;
    }
    eprintln!("Meshcraft: X11 display has no DRI3 presentation support");
    retry_with_software()
}

fn software_driver(failed: bool, explicit: bool, directories: &[PathBuf]) -> Option<PathBuf> {
    if !failed || explicit {
        return None;
    }
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name == "lvp_icd.json"
                                || name == format!("lvp_icd.{}.json", env::consts::ARCH)
                        })
            })
            .collect();
        paths.sort();
        if let Some(path) = paths.into_iter().next() {
            return Some(path);
        }
    }
    None
}

/// Retry only failed graphics initialization in a fresh process with Mesa's CPU driver.
/// Driver changes are scoped to the child; healthy hardware and explicit driver choices
/// are preserved, and the private retry marker prevents restart loops.
pub fn retry_with_software() -> Option<std::io::Result<std::process::ExitStatus>> {
    let explicit = [
        "VK_ICD_FILENAMES",
        "VK_DRIVER_FILES",
        "VK_ADD_DRIVER_FILES",
        "MESHCRAFT_GRAPHICS_RETRY",
    ]
    .iter()
    .any(|name| env::var_os(name).is_some());
    let mut directories = vec![
        PathBuf::from("/usr/share/vulkan/icd.d"),
        PathBuf::from("/etc/vulkan/icd.d"),
    ];
    if let Some(data) = env::var_os("XDG_DATA_DIRS") {
        directories.extend(env::split_paths(&data).map(|p| p.join("vulkan/icd.d")));
    }
    let path = software_driver(true, explicit, &directories)?;
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => return Some(Err(error)),
    };
    eprintln!(
        "Meshcraft: retrying graphics with Mesa software Vulkan ({})",
        path.display()
    );
    let mut command = std::process::Command::new(executable);
    command
        .args(env::args_os().skip(1))
        .env("VK_ICD_FILENAMES", &path)
        .env("VK_DRIVER_FILES", &path)
        .env("MESHCRAFT_GRAPHICS_RETRY", "1");
    if env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none() {
        command.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    Some(command.status())
}

/// Display an honest startup error without requiring GPUI, OpenGL, Vulkan or a portal.
pub fn show_failure(error: &str) -> anyhow::Result<()> {
    let (connection, screen) = x11rb::connect(None)?;
    let screen = &connection.setup().roots[screen];
    let window = connection.generate_id()?;
    let gc = connection.generate_id()?;
    let font = connection.generate_id()?;
    let width = 740u16.min(screen.width_in_pixels);
    let height = 250u16.min(screen.height_in_pixels);
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            screen.root,
            0,
            0,
            width,
            height,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            0,
            &xproto::CreateWindowAux::new()
                .background_pixel(screen.white_pixel)
                .event_mask(
                    xproto::EventMask::EXPOSURE
                        | xproto::EventMask::KEY_PRESS
                        | xproto::EventMask::BUTTON_PRESS
                        | xproto::EventMask::STRUCTURE_NOTIFY,
                ),
        )?
        .check()?;
    connection.change_property8(
        xproto::PropMode::REPLACE,
        window,
        xproto::AtomEnum::WM_NAME,
        xproto::AtomEnum::STRING,
        b"Meshcraft - Graphics initialization error",
    )?;
    connection.change_property8(
        xproto::PropMode::REPLACE,
        window,
        xproto::AtomEnum::WM_CLASS,
        xproto::AtomEnum::STRING,
        b"meshcraft\0Meshcraft\0",
    )?;
    let protocols = connection
        .intern_atom(false, b"WM_PROTOCOLS")?
        .reply()?
        .atom;
    let close = connection
        .intern_atom(false, b"WM_DELETE_WINDOW")?
        .reply()?
        .atom;
    connection.change_property32(
        xproto::PropMode::REPLACE,
        window,
        protocols,
        xproto::AtomEnum::ATOM,
        &[close],
    )?;
    connection.open_font(font, b"fixed")?.check()?;
    connection
        .create_gc(
            gc,
            window,
            &xproto::CreateGCAux::new()
                .foreground(screen.black_pixel)
                .background(screen.white_pixel)
                .font(font),
        )?
        .check()?;
    connection.map_window(window)?;
    connection.flush()?;
    let reason = if error.contains("NoSupportedDeviceFound") {
        "No usable Vulkan graphics device was found."
    } else if error.contains("PlatformNotSupported") {
        "The display server could not present the graphics surface."
    } else {
        "The graphics backend reported an initialization error."
    };
    let lines = [
        "Meshcraft could not initialize its graphics renderer.",
        "",
        "Enable a Vulkan-capable graphics driver for your desktop.",
        "For virtual desktops, install Mesa's software Vulkan driver.",
        "Ubuntu / Debian package: mesa-vulkan-drivers",
        "",
        reason,
        "",
        "Press any key, click this window, or close it to exit.",
    ];
    loop {
        match connection.wait_for_event()? {
            Event::Expose(_) => {
                for (index, text) in lines.iter().enumerate() {
                    let mut item = vec![text.len() as u8, 0];
                    item.extend_from_slice(text.as_bytes());
                    connection.poly_text8(window, gc, 16, 30 + index as i16 * 22, &item)?;
                }
                connection.flush()?;
            }
            Event::KeyPress(_) | Event::ButtonPress(_) | Event::DestroyNotify(_) => break,
            Event::ClientMessage(event)
                if event.type_ == protocols && event.data.as_data32()[0] == close =>
            {
                break;
            }
            _ => {}
        }
    }
    connection.destroy_window(window)?;
    connection.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hardware_and_explicit_driver_choices_are_preserved() {
        assert!(software_driver(false, false, &[]).is_none());
        assert!(software_driver(true, true, &[]).is_none());
    }
    #[test]
    fn virtual_desktops_select_only_an_existing_lavapipe_manifest() {
        let directory = env::temp_dir().join(format!("meshcraft-lvp-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("intel_icd.json"), "{}").unwrap();
        assert!(software_driver(true, false, std::slice::from_ref(&directory)).is_none());
        let filename = format!("lvp_icd.{}.json", env::consts::ARCH);
        std::fs::write(directory.join(&filename), "{}").unwrap();
        assert_eq!(
            software_driver(true, false, std::slice::from_ref(&directory)),
            Some(directory.join(filename))
        );
        for (failed, explicit) in [(false, false), (false, true), (true, true)] {
            assert!(software_driver(failed, explicit, std::slice::from_ref(&directory)).is_none());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
