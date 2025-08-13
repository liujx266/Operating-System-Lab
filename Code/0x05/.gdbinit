file esp/KERNEL.ELF
# 更改连接方式，使用标准格式
target remote localhost:1234
# 如果上面的命令不工作，可以尝试以下命令
# gef-remote localhost:1234
tmux-setup
b ysos_kernel::init