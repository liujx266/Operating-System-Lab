file esp/KERNEL.ELF
gef config context.layout "-legend regs -stack code -args source -threads -trace extra memory"
target remote localhost:1234
tmux-setup
b ysos_kernel::init