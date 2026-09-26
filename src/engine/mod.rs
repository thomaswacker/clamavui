pub mod locate;
pub mod scan;
pub mod signatures;
pub mod update;

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

/// Test-only helpers shared by the engine modules.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard};

    static EXEC_LOCK: Mutex<()> = Mutex::new(());

    /// Serialize tests that write a script and execute it. A concurrent `fork` in another
    /// test can inherit the script's write descriptor for a moment, and `exec` then fails
    /// with "Text file busy"; holding this lock across write + spawn avoids that race.
    pub fn exec_lock() -> MutexGuard<'static, ()> {
        EXEC_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
