use core::alloc::Layout;

use crate::proc::*;
use crate::drivers::filesystem;

use super::SyscallArgs;

pub fn spawn_process(args: &SyscallArgs) -> usize {
    // 获取文件路径或应用名称
    let ptr = args.arg0 as *const u8;
    let len = args.arg1;

    if ptr.is_null() || len == 0 {
        return 0;
    }

    // 将输入参数转换为字符串
    let path = unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    // 使用修改后的spawn函数（现在支持文件路径和应用名称）
    match spawn(path) {
        Some(pid) => pid.0 as usize,
        None => 0,
    }
}

pub fn sys_write(args: &SyscallArgs) -> usize {
    let fd = args.arg0 as u8;
    let ptr = args.arg1 as *const u8;
    let len = args.arg2;
    
    if ptr.is_null() || len == 0 {
        return 0;
    }
    
    // 将指针和长度转换为切片
    let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
    
    // 调用进程的write函数
    let result = write(fd, buf);
    
    // 如果结果为负数，返回0，否则返回写入的字节数
    if result.is_negative() {
        0
    } else {
        result as usize
    }
}

pub fn sys_read(args: &SyscallArgs) -> usize {
    let fd = args.arg0 as u8;
    let ptr = args.arg1 as *mut u8;
    let len = args.arg2;
    
    if ptr.is_null() || len == 0 {
        return 0;
    }
    
    // 将指针和长度转换为可变切片
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    
    // 调用进程的read函数
    let result = read(fd, buf);
    
    // 如果结果为负数，返回0，否则返回读取的字节数
    if result.is_negative() {
        0
    } else {
        result as usize
    }
}

pub fn exit_process(args: &SyscallArgs, context: &mut ProcessContext) {
    // 使用返回码退出进程
    let ret_code = args.arg0 as isize;
    exit(ret_code, context);
}

pub fn list_process() {
    // 列出所有进程
    print_process_list();
}

pub fn sys_allocate(args: &SyscallArgs) -> usize {
    let layout = unsafe { (args.arg0 as *const Layout).as_ref().unwrap() };

    if layout.size() == 0 {
        return 0;
    }

    let ret = crate::memory::user::USER_ALLOCATOR
        .lock()
        .allocate_first_fit(*layout);

    match ret {
        Ok(ptr) => ptr.as_ptr() as usize,
        Err(_) => 0,
    }
}

pub fn sys_deallocate(args: &SyscallArgs) {
    let layout = unsafe { (args.arg1 as *const Layout).as_ref().unwrap() };

    if args.arg0 == 0 || layout.size() == 0 {
        return;
    }

    let ptr = args.arg0 as *mut u8;

    unsafe {
        crate::memory::user::USER_ALLOCATOR
            .lock()
            .deallocate(core::ptr::NonNull::new_unchecked(ptr), *layout);
    }
}

pub fn sys_getpid(_args: &SyscallArgs) -> usize {
    // 使用当前函数获取进程ID，而不是直接使用processor模块
    let pid = crate::proc::get_process_manager().current().pid();
    pid.0 as usize
}

pub fn sys_waitpid(args: &SyscallArgs) -> usize {
    let pid = ProcessId(args.arg0 as u16);

    // 检查进程是否存活
    if !still_alive(pid) {
        // 如果进程已退出，尝试获取退出码
        match get_exit_code(pid) {
            Some(code) => code as usize,
            None => 0, // 进程不存在或已被回收
        }
    } else {
        // 进程仍在运行，返回特殊值表示正在运行
        usize::MAX  // 使用最大的usize值表示进程仍在运行
    }
}

pub fn list_dir(args: &SyscallArgs) {
    let ptr = args.arg0 as *const u8;
    let len = args.arg1;

    if ptr.is_null() || len == 0 {
        return;
    }

    // 将输入参数转换为字符串
    let path = unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => return,
        }
    };

    // 调用文件系统的ls函数
    filesystem::ls(path);
}

pub fn sys_open(args: &SyscallArgs) -> usize {
    let ptr = args.arg0 as *const u8;
    let len = args.arg1;

    if ptr.is_null() || len == 0 {
        return 0;
    }

    // 将输入参数转换为字符串
    let path = unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };

    // 使用进程模块的open_file函数
    match open_file(path) {
        Ok(fd) => fd as usize,
        Err(_) => 0,
    }
}

pub fn sys_close(args: &SyscallArgs) -> usize {
    let fd = args.arg0 as u8;

    // 使用进程模块的close_file函数
    if close_file(fd) {
        0 // 成功
    } else {
        1 // 失败
    }
}
