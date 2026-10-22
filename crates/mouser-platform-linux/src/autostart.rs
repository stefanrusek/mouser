//! XDG autostart: `~/.config/autostart/com.mouser.app.desktop` (FR-011).

use mouser_platform::{Autostart, Result};
use std::path::{Path, PathBuf};

pub(crate) struct XdgAutostart {
    file: PathBuf,
    exe: PathBuf,
}

impl XdgAutostart {
    pub(crate) fn new(config_dir: PathBuf, exe: &Path) -> Self {
        XdgAutostart {
            file: config_dir.join("autostart/com.mouser.app.desktop"),
            exe: exe.into(),
        }
    }
}

impl Autostart for XdgAutostart {
    fn enabled(&self) -> bool {
        self.file.exists()
    }

    fn set(&mut self, on: bool) -> Result<()> {
        if on {
            std::fs::create_dir_all(self.file.parent().expect("autostart dir"))
                .expect("create autostart dir");
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=Mouser\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n",
                self.exe.display()
            );
            std::fs::write(&self.file, entry).expect("write autostart entry");
        } else if self.file.exists() {
            std::fs::remove_file(&self.file).expect("remove autostart entry");
        }
        Ok(())
    }
}
