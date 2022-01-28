use super::process::{Process, Thread};
use crate::arch::{apic, cpu, interrupts};
use crate::fs::vfs;
use crate::serial;
use alloc::collections::VecDeque;
use alloc::rc::Rc;
use core::cell::RefCell;

static mut SCHEDULER: Option<Scheduler> = None;

pub struct SchedulerQueues {
    pub runnable: VecDeque<Rc<RefCell<Thread>>>,
    pub waiting: VecDeque<Rc<RefCell<Thread>>>,
}

pub struct Scheduler {
    pub queues: SchedulerQueues,
    pub running_thread: Option<Rc<RefCell<Thread>>>,
}

impl Scheduler {}

interrupts::isr!(reschedule, |regs| {
    let scheduler = get();

    if let Some(thread) = scheduler.queues.runnable.pop_front() {
        if let Some(previous_thread) = scheduler.running_thread.clone() {
            previous_thread.borrow_mut().regs = *regs;
            scheduler.queues.runnable.push_back(previous_thread);
        }

        scheduler.running_thread = Some(thread);
        let running_thread = scheduler.running_thread.as_ref().unwrap().borrow();

        running_thread.parent.borrow().pagemap.switch_pagemap();

        apic::get().eoi();
        Thread::switch(&running_thread.regs);

        unreachable!();
    } else {
        if let Some(_) = scheduler.running_thread.as_ref() {
            apic::get().eoi();
            cpu::halt();
        }

        unreachable!();
    }
});

pub fn init() {
    let vector =
        interrupts::alloc_vector().expect("Could not allocate an interrupt vector for the scheduler");
    unsafe {
        interrupts::register_isr(vector, reschedule as u64, 0, 0x8e);
    }
    apic::get().calibrate_timer(30, vector);
}

pub fn get() -> &'static mut Scheduler {
    unsafe {
        SCHEDULER
            .as_mut()
            .expect("The scheduler hasn't been initialized")
    }
}
