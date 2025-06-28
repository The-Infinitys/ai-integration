use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

pub(crate) static AI_PRINT_ENABLED: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

/// コマンドライン引数に--ai-printが含まれていれば有効化
pub fn init_ai_print_flag() {
    INIT.call_once(|| {
        let enabled = std::env::args().any(|arg| arg == "--ai-print");
        AI_PRINT_ENABLED.store(enabled, Ordering::Relaxed);
    });
}

#[macro_export]
macro_rules! ai_print {
    ($($arg:tt)*) => {
        if $crate::utils::ai_print::AI_PRINT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            print!($($arg)*);
        }
    };
}#[macro_export]
macro_rules! ai_println {
    ($($arg:tt)*) => {
        if $crate::utils::ai_print::AI_PRINT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            println!($($arg)*);
        }
    };
}