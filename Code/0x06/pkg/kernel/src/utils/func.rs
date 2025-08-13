pub fn test() -> ! {
    let mut count = 0;
    let id;
    
    // 获取线程ID并打印启动消息
    if let Some(id_env) = crate::proc::env("id") {
        id = id_env;
        println!("Thread #{} started", id);
    } else {
        id = "unknown".into();
        println!("Thread #unknown started");
    }
    
    // 为不同进程设置不同的起始计数，避免它们在相同时间输出
    // 使用字符串的第一个字符的ASCII值作为偏移量
    let id_offset = if !id.is_empty() {
        (id.chars().next().unwrap() as u64) % 5000
    } else {
        0
    };
    count = id_offset as usize;
    
    loop {
        // 定期展示进程活动状态
        count += 1;
        
        // 降低输出频率并使不同线程错开输出时间，减少混乱
        if count % 20000 == 0 {
            // 每20000次循环输出一次，并且线程之间有偏移，不会同时输出
            let msg = match count / 20000 % 4 {
                0 => " -> ",
                1 => " => ",
                2 => " -- ",
                _ => " <> ",
            };
            
            // 使用线程ID作为前缀，增强可读性区分
            println!("Thread#{}{}{}", id, msg, "Tick!");
            
            // 如果计数太大，重置它但保留偏移量
            if count >= 1000000 {
                count = id_offset as usize;
            }
        }
        
        // 让出CPU时间，允许其他进程执行
        x86_64::instructions::hlt();
    }
}

#[inline(never)]
fn huge_stack() {
    println!("Huge stack testing...");

    let mut stack = [0u64; 0x1000];

    for (idx, item) in stack.iter_mut().enumerate() {
        *item = idx as u64;
    }

    for i in 0..stack.len() / 256 {
        println!("{:#05x} == {:#05x}", i * 256, stack[i * 256]);
    }
}

pub fn stack_test() -> ! {
    huge_stack();
    crate::proc::process_exit(0)
}
