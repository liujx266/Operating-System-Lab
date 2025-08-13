use crate::alloc::string::ToString;
use crate::errln;
// 正确导入sys_exit函数
use crate::sys_exit;

#[macro_export]
macro_rules! entry {
    ($fn:ident) => {
        #[unsafe(export_name = "_start")]
        pub extern "C" fn __impl_start() {
            let ret = $fn();
            // 使用宏内部的$crate引用
            $crate::sys_exit(ret);
            // 不可达代码
            #[allow(unreachable_code)]
            loop {}
        }
    };
}

#[cfg_attr(not(test), panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let location = if let Some(location) = info.location() {
        alloc::format!(
            "{}@{}:{}",
            location.file(),
            location.line(),
            location.column()
        )
    } else {
        "Unknown location".to_string()
    };

    errln!(
        "\n\n\rERROR: panicked at {}\n\n\r{}",
        location,
        info.message()
    );

    // 在panic函数中使用正确导入的函数
    sys_exit(1);
    
    // 不可达代码
    #[allow(unreachable_code)]
    loop {}
}
