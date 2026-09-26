pub mod locate;
pub mod scan;
pub mod signatures;

use std::path::Path;
use std::process::Command;

/// Build a `Command` that never opens a console window on Windows.
pub fn quiet_command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
