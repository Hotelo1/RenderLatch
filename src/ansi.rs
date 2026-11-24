// Cor e formatacao de console (Windows 10+ com VT habilitado).

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

pub fn init() {
    unsafe { crate::win::console_utf8() };
    let tty = std::io::stdout().is_terminal();
    ON.store(tty, Ordering::Relaxed);
    if tty {
        unsafe { crate::win::enable_vt() };
    }
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

fn wrap(code: &str, s: &str) -> String {
    if enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold<S: AsRef<str>>(s: S) -> String {
    wrap("1", s.as_ref())
}
pub fn dim<S: AsRef<str>>(s: S) -> String {
    wrap("2", s.as_ref())
}
pub fn red<S: AsRef<str>>(s: S) -> String {
    wrap("31", s.as_ref())
}
pub fn grn<S: AsRef<str>>(s: S) -> String {
    wrap("32", s.as_ref())
}
pub fn yel<S: AsRef<str>>(s: S) -> String {
    wrap("33", s.as_ref())
}
pub fn cya<S: AsRef<str>>(s: S) -> String {
    wrap("36", s.as_ref())
}
pub fn inv<S: AsRef<str>>(s: S) -> String {
    wrap("7", s.as_ref())
}

/// Largura visivel de uma string (ignora escapes ANSI).
pub fn vlen(s: &str) -> usize {
    let mut n = 0usize;
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch == 'm' {
                esc = false;
            }
            continue;
        }
        if ch == '\x1b' {
            esc = true;
            continue;
        }
        n += 1;
    }
    n
}

pub fn pad<S: AsRef<str>>(s: S, w: usize) -> String {
    let s = s.as_ref();
    let l = vlen(s);
    if l >= w {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(w - l))
    }
}
