use log::{Metadata, Record, Level, LevelFilter};
use crate::println;

/// 解析日志级别字符串，返回对应的 LevelFilter
fn parse_log_level(level: &str) -> LevelFilter {
    // 使用 eq_ignore_ascii_case 替代 to_lowercase
    let level = level.trim();
    
    if level.eq_ignore_ascii_case("off") {
        LevelFilter::Off
    } else if level.eq_ignore_ascii_case("error") {
        LevelFilter::Error
    } else if level.eq_ignore_ascii_case("warn") {
        LevelFilter::Warn
    } else if level.eq_ignore_ascii_case("info") {
        LevelFilter::Info
    } else if level.eq_ignore_ascii_case("debug") {
        LevelFilter::Debug
    } else if level.eq_ignore_ascii_case("trace") {
        LevelFilter::Trace
    } else {
        // 如果无法解析，默认使用 Info 级别，并打印警告
        println!("\x1b[33m[WARN ]\x1b[0m Unknown log level: {}, using 'info'", level);
        LevelFilter::Info
    }
}

pub fn init(log_level: &str) {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).unwrap();
    
    // 根据启动配置参数设置日志级别
    let level = parse_log_level(log_level);
    log::set_max_level(level);
    
    info!("Logger Initialized with level: {}", log_level);
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // 这里可以根据模块名称或其他条件进行更细粒度的过滤
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        // 只处理启用的日志记录
        if self.enabled(record.metadata()) {
            
            // 根据日志级别添加不同的颜色和前缀
            let (color_code, level_str) = match record.level() {
                Level::Error => ("\x1b[31m", "ERROR"), // 红色
                Level::Warn => ("\x1b[33m", "WARN "), // 黄色
                Level::Info => ("\x1b[32m", "INFO "), // 绿色
                Level::Debug => ("\x1b[36m", "DEBUG"), // 青色
                Level::Trace => ("\x1b[90m", "TRACE"), // 灰色
            };
            
            // 简化输出：只显示级别和消息内容
            println!(
                "[{}{}{}] {}",
                color_code, level_str, "\x1b[0m",
                record.args()
            );
        }
    }

    fn flush(&self) {
        // 串口不需要刷新缓冲区
    }
}
