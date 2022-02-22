use crate::arch::{cpu, mm::pmm};
use crate::fs::vfs;
use crate::mm::vmm;
use crate::serial;
use crate::utils::bitmap;
use alloc::{rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;
use core::arch::asm;

pub const MAX_FDS_PER_PROCESS: usize = 128;

static mut PID_BITMAP: Option<bitmap::Bitmap> = None;
static mut TID_BITMAP: Option<bitmap::Bitmap> = None;

#[derive(PartialEq, Clone, Copy)]
pub enum Status {
    Running,
    Waiting,
    Dying,
}

#[repr(u64)]
#[derive(Clone, Copy)]
pub enum SelectorValues {
    KernelCs = 0x8,
    KernelDs = 0x10,

    // the RPL for the following selectors is 0x3
    UserCs = 0x1b,
    UserDs = 0x23,
}

pub struct Process {
    pub pid: usize,
    pub status: Status,
    pub name: String,
    pub pagemap: Option<vmm::VirtualMemManager>,
    pub threads: Vec<Rc<RefCell<Thread>>>,
    pub file_desc_list: [Option<vfs::FileDescription>; MAX_FDS_PER_PROCESS],
    pub working_dir: Option<vfs::FileDescription>,
}

impl Process {
    pub fn new(name: String, rip: u64, working_dir: Option<vfs::FileDescription>) -> Rc<RefCell<Self>> {
        // serial::print!("hey!\n");
        // let pagemap = vmm::VirtualMemManager::new(true);
        // serial::print!("pagemap: {:#x}\n", pagemap.pagemap.as_u64());
        // let pid = Process::alloc_pid().unwrap();
        // serial::print!("pid: {}\n", pid);
        const NO_FD: Option<vfs::FileDescription> = None;
        // serial::print!("uh here\n");
        let new_proc = Process {
            pid: 0,
            status: Status::Running,
            name,
            pagemap: None,
            threads: Vec::new(),
            file_desc_list: [NO_FD; MAX_FDS_PER_PROCESS],
            working_dir,
        };

        // serial::print!("ok thread now\n");
        // let main_thread = Thread::new(rip, SelectorValues::UserCs, new_proc.clone());
        // new_proc.borrow_mut().threads.push(main_thread);
        serial::print!("a\n");
        Rc::new(RefCell::new(new_proc))
    }

    pub fn alloc_pid() -> Option<usize> {
        let bitmap = unsafe {
            PID_BITMAP
                .as_mut()
                .expect("Pid bitmap hasn't been initialized")
        };
       
        for i in 0..bitmap.size() * 8 {
            if !bitmap.is_set(i) {
                bitmap.set(i);
                return Some(i);
            }
        }

        None
    }
}

pub struct Thread {
    pub tid: usize,
    pub status: Status,
    pub parent: Rc<RefCell<Process>>,
    pub kernel_stack: u64,
    pub regs: cpu::InterruptContext,
}

impl Thread {
    pub fn new(rip: u64, cs: SelectorValues, parent: Rc<RefCell<Process>>) -> Rc<RefCell<Self>> {
        serial::print!("thread new\n");
        let mut new_thread = Thread {
            tid: Self::alloc_tid().expect("Could not allocate a new tid"),
            status: Status::Running,
            parent,
            kernel_stack: 0,
            regs: cpu::InterruptContext::default(),
        };

        if cs as u64 & 0x3 != 0 {
            // userspace thread
            // TODO: allocate the stack and mmap it
            new_thread.regs.ss = SelectorValues::UserDs as u64;
        } else {
            new_thread.regs.ss = SelectorValues::KernelDs as u64;
        }

        new_thread.regs.rflags = 0x202;
        new_thread.regs.cs = cs as u64;
        new_thread.regs.rip = rip;
        serial::print!("all good at new thread\n");
        Rc::new(RefCell::new(new_thread))
    }

    pub fn alloc_tid() -> Option<usize> {
        let mut bitmap = unsafe {
            TID_BITMAP
                .as_mut()
                .expect("Tid bitmap hasn't been initialized")
        };

        for i in 0..bitmap.size() * 8 {
            if !bitmap.is_set(i) {
                bitmap.set(i);
                return Some(i);
            }
        }

        None
    }

    // #[naked]
    // pub unsafe extern "C" fn switch(regs: &cpu::InterruptContext) {
    //     asm!(
    //         "mov rsp, rdi",
    //         "pop rax",
    //         "pop rbx",
    //         "pop rcx",
    //         "pop rdx",
    //         "pop rsi",
    //         "pop rdi",
    //         "pop rbp",
    //         "pop r8",
    //         "pop r9",
    //         "pop r10",
    //         "pop r11",
    //         "pop r12",
    //         "pop r13",
    //         "pop r14",
    //         "pop r15",
    //         "iretq",
    //         options(noreturn)
    //     )
    // }

    // pub fn block(&self) {
    //     // if self.status == Status::Waiting {
    //     //     return;
    //     // }

    //     // let res = scheduler::get()
    //     //     .queues
    //     //     .runnable
    //     //     .binary_search_by(|thread| thread.tid.cmp(&self.tid));

    //     // if let Ok(index) = res {
    //     //     scheduler::get().queues.runnable.remove(index);
    //     //     scheduler::get().queues.waiting.insert(index, value)
    //     // } else {
    //     //     // error
    //     // }
    // }
}

/*
    let buffer = alloc()
    waiting_threads.push(self)
    self.block()

    keyboard_handler(key) {
        if key == enter {
            for t in waiting_threads {
                t.unblock()
            }
        }
        buffer[i] = key
    }



*/

pub unsafe fn init_bitmaps() {
    let a = bitmap::Bitmap::new(pmm::PAGE_SIZE as usize);
    let b = bitmap::Bitmap::new(pmm::PAGE_SIZE as usize);
    serial::print!("a: {:p}, b: {:p}\n", &a, &b);
    PID_BITMAP = Some(a);
    TID_BITMAP = Some(b);
}
