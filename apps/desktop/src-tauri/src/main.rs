// Keeps the console window from appearing alongside the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Before Tauri, before the runtime, before anything draws: `ssh` re-executes
    // this binary as its `SSH_ASKPASS` helper, and in that role the process
    // exists only to carry one prompt to the running app and one answer back.
    // Checking the socket variable rather than an argv flag is what makes it
    // safe to point `SSH_ASKPASS` straight at this binary — ssh calls the
    // helper with the prompt as argv[1] and no flag of our choosing.
    #[cfg(unix)]
    if let Ok(socket) = std::env::var(onlydiffs_lib::services::ssh::askpass::ASKPASS_SOCKET_ENV) {
        onlydiffs_lib::services::ssh::askpass::helper_main(&socket);
    }

    onlydiffs_lib::run()
}
