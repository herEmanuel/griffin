use super::scheduler;
use crate::arch::{cpu, mm::pmm};
use crate::fs::vfs;
use crate::mm::vmm;
use crate::utils::bitmap;
use alloc::{rc::Rc, string::String, vec::Vec};
use core::cell::RefCell;
use spin::{Lazy, Mutex};

const MAX_FDS_PER_PROCESS: usize = 128;

#[derive(PartialEq)]
pub enum Status {
    Running,
    Waiting,
    Dying,
}

#[repr(u64)]
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
    pub pagemap: vmm::VirtualMemManager,
    pub threads: Vec<Rc<RefCell<Thread>>>,
    pub file_desc_list: [Option<vfs::FileDescription>; MAX_FDS_PER_PROCESS],
    pub working_dir: vfs::FileDescription,
}

impl Process {
    pub fn new(name: String, rip: u64, working_dir: vfs::FileDescription) -> Rc<RefCell<Self>> {
        let pagemap = vmm::VirtualMemManager::new(true);

        const NO_FD: Option<vfs::FileDescription> = None;

        let new_proc = Rc::new(RefCell::new(Process {
            pid: Process::alloc_pid().expect("Could not allocate a new pid"),
            status: Status::Running,
            name,
            pagemap,
            threads: Vec::new(),
            file_desc_list: [NO_FD; MAX_FDS_PER_PROCESS],
            working_dir,
        }));

        // let main_thread = Thread::new(new_proc.clone());
        // new_proc.borrow_mut().threads.push(main_thread);

        new_proc
    }

    pub fn alloc_pid() -> Option<usize> {
        static PID_BITMAP: Lazy<Mutex<bitmap::Bitmap>> =
            Lazy::new(|| Mutex::new(bitmap::Bitmap::new(pmm::PAGE_SIZE as usize)));

        let mut bitmap = PID_BITMAP.lock();

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
    pub regs: cpu::InterruptContext,
}

impl Thread {
    pub fn new(rip: u64, cs: u64, parent: Rc<RefCell<Process>>) -> Rc<RefCell<Self>> {
        let mut new_thread = Thread {
            tid: Self::alloc_tid().expect("Could not allocate a new tid"),
            status: Status::Running,
            parent,
            regs: cpu::InterruptContext::default(),
        };

        if cs & 0x3 != 0 {
            // userspace thread
            // TODO: allocate the stack and mmap it
            new_thread.regs.ss = SelectorValues::UserDs as u64;
        } else {
            new_thread.regs.ss = SelectorValues::KernelDs as u64;
        }

        new_thread.regs.rflags = 0x202;
        new_thread.regs.cs = cs;
        new_thread.regs.rip = rip;

        Rc::new(RefCell::new(new_thread))
    }

    pub fn alloc_tid() -> Option<usize> {
        static TID_BITMAP: Lazy<Mutex<bitmap::Bitmap>> =
            Lazy::new(|| Mutex::new(bitmap::Bitmap::new(pmm::PAGE_SIZE as usize)));

        let mut bitmap = TID_BITMAP.lock();

        for i in 0..bitmap.size() * 8 {
            if !bitmap.is_set(i) {
                bitmap.set(i);
                return Some(i);
            }
        }

        None
    }

    #[naked]
    pub unsafe fn switch(regs: &cpu::InterruptContext) {
        asm!(
            "mov rsp, rdi",
            "pop rax",
            "pop rbx",
            "pop rcx",
            "pop rdx",
            "pop rsi",
            "pop rdi",
            "pop rbp",
            "pop r8",
            "pop r9",
            "pop r10",
            "pop r11",
            "pop r12",
            "pop r13",
            "pop r14",
            "pop r15",
            "iretq",
            options(noreturn)
        )
    }

    pub fn block(&self) {
        // if self.status == Status::Waiting {
        //     return;
        // }

        // let res = scheduler::get()
        //     .queues
        //     .runnable
        //     .binary_search_by(|thread| thread.tid.cmp(&self.tid));

        // if let Ok(index) = res {
        //     scheduler::get().queues.runnable.remove(index);
        //     scheduler::get().queues.waiting.insert(index, value)
        // } else {
        //     // error
        // }
    }
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
