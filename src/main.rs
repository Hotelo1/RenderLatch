#![cfg_attr(renderlatch_release, windows_subsystem = "windows")]

mod ansi;
mod cmds;
mod cpu;
mod distribution;
mod dlls;
mod dlss5;
mod gpu;
mod json;
mod machine;
mod plan;
mod report;
mod win;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(renderlatch_release)]
    if args.first().is_some_and(|arg| arg != "gui") {
        #[link(name = "kernel32")]
        extern "system" { fn AttachConsole(process_id: u32) -> i32; }
        // A console invocation can print normally. Redirected standard handles stay usable.
        unsafe { AttachConsole(u32::MAX); }
    }
    ansi::init();
    let code = cmds::dispatch(&args);
    std::process::exit(code);
}
