// デバッグ出力用のマクロ
#[macro_export]
macro_rules! dprint {
    ($debug_mode:expr, $($arg:tt)*) => {
        if $debug_mode {
            print!($($arg)*);
        }
    };
}
#[macro_export]
macro_rules! dprintln {
    ($debug_mode:expr, $($arg:tt)*) => {
        if $debug_mode {
            println!($($arg)*);
        }
    };
}
