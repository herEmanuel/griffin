pub unsafe fn halt() -> ! {
    loop {
        asm!("hlt");
    }
}
